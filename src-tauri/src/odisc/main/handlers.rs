use crate::odisc::main::helpers::Mapping;
use regex::Regex;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use std::io;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::net::UdpSocket;

// OSC

pub async fn incoming_osc_handler(sock: &UdpSocket, buf: &mut [u8]) -> io::Result<OscPacket> {
    let (len, addr) = sock.recv_from(buf).await?;
    println!("{:?} bytes received from {:?}", len, addr);

    let _ = sock.send_to(&buf[..len], addr).await?;
    let (_rest, packet) = decoder::decode_udp(&buf[..len]).unwrap();

    Ok(packet)
}

pub async fn outgoing_osc_handler(
    sock: &UdpSocket,
    osc_out_address: &str,
    osc_out_args: Option<&str>,
    osc_host: &str,
    osc_port: &u16,
    app_handle: &AppHandle,
) -> std::io::Result<()> {
    let final_args: Vec<OscType> = if let Some(osc_out_args) = osc_out_args {
        if !osc_out_args.trim().is_empty() {
            osc_out_args
                .split_whitespace()
                .map(|arg| {
                    if let Ok(num) = arg.parse::<f32>() {
                        OscType::Float(num)
                    } else {
                        OscType::String(arg.to_string())
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Build the OSC message
    let msg = OscMessage {
        addr: osc_out_address.to_string(),
        args: final_args,
    };
    let packet = OscPacket::Message(msg);

    // Encode the packet to bytes
    let encoded = encoder::encode(&packet).unwrap();

    // Send the packet over UDP
    let addr = format!("{}:{}", osc_host, osc_port);
    sock.send_to(&encoded, addr).await?;

    println!(
        "Sent OSC message: {} {:#?}",
        osc_out_address,
        osc_out_args.unwrap()
    );
    app_handle
        .emit(
            "backend-log",
            format!(
                "Sent OSC message: {} {:#?}",
                osc_out_address,
                osc_out_args.unwrap()
            ),
        )
        .unwrap();

    Ok(())
}

// CSV MAPPING

pub fn match_mappings<'a>(
    mappings: &'a [Mapping],
    msg: &OscMessage,
    app_handle: &AppHandle,
) -> Option<&'a Mapping> {
    if let Some(mapping) = mappings.iter().find(|m| {
        let addr_match: bool = m.osc_in_address == msg.addr;

        let args_match = match &m.osc_in_args {
            None => true,
            Some(s) if s.is_empty() => true,
            Some(expected) => {
                if msg.args.len() != 1 {
                    false
                } else if let rosc::OscType::String(ref val) = msg.args[0] {
                    val == expected
                } else {
                    false
                }
            }
        };

        addr_match && args_match
    }) {
        println!(
            "Found mapping: {:?} {:?}",
            mapping.osc_in_address,
            mapping.osc_in_args.as_deref()
        );
        app_handle
            .emit(
                "backend-log",
                format!(
                    "Found mapping: {:?} {:?}",
                    mapping.osc_in_address,
                    mapping.osc_in_args.as_deref()
                ),
            )
            .unwrap();
        return Some(mapping);
    } else {
        println!("No mapping found for {:?}", msg.addr);
        return None;
    }
}

// HANDLE QC

fn parse_preset_id(preset_id: &str, app_handle: &AppHandle) -> Option<(u32, char)> {
    let re = Regex::new(r"^(\d+)([A-H])$").unwrap();
    if let Some(caps) = re.captures(preset_id) {
        let number = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let letter = caps.get(2).and_then(|m| m.as_str().chars().next());
        if let (Some(number), Some(letter)) = (number, letter) {
            Some((number, letter))
        } else {
            eprintln!(
                "Invalid Quad Cortex preset format: {}. Expected format like '1A', '12D', etc.",
                preset_id
            );
            app_handle
                .emit(
                    "backend-log",
                    format!(
                         "Invalid Quad Cortex preset format: {}. Expected format like '1A', '12D', etc.",
                preset_id
                    ),
                )
                .unwrap();
            None
        }
    } else {
        eprintln!(
            "Invalid Quad Cortex preset format: {}. Expected format like '1A', '12D', etc.",
            preset_id
        );
        app_handle
            .emit(
                "backend-log",
                format!(
                    "Invalid Quad Cortex preset format: {}. Expected format like '1A', '12D', etc.",
                    preset_id
                ),
            )
            .unwrap();
        None
    }
}

fn parse_preset_midi(number: &u32, letter: &char) -> Option<u32> {
    if number < &1u32 || number > &32u32 {
        eprintln!("Invalid bank number: {}. Must be between 1 and 32.", number);
        return None;
    }

    let preset_offset: u32 = (*letter as u32) - ('A' as u32);
    let pgm_ch_num = (number - 1) * 8 + preset_offset;

    return Some(pgm_ch_num);
}

pub fn send_qc_preset(preset_id: &String, setlist: &u32, app_handle: &AppHandle) -> Option<u32> {
    if let Some((number, letter)) = parse_preset_id(preset_id, app_handle) {
        let program_change_number = parse_preset_midi(&number, &letter);
        println!(
            "Sending QC Preset: Setlist {}, Preset {} -> PC: {}",
            setlist,
            preset_id,
            program_change_number.unwrap()
        );
        app_handle
            .emit(
                "backend-log",
                format!(
                    "Sending QC Preset: Setlist {}, Preset {} -> PC: {}",
                    setlist,
                    preset_id,
                    program_change_number.unwrap()
                ),
            )
            .unwrap();
        return program_change_number;
    } else {
        // Handle the error case if needed
        return None;
    }
}
