use crate::odisc::main::custom_print;
use crate::odisc::main::helpers::Mapping;
use crate::odisc::main::Output;
use regex::Regex;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use std::io;
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

    let _ = custom_print(
        format!(
            "Sent OSC message: {} {:#?}",
            osc_out_address,
            osc_out_args.unwrap()
        ),
        Output::App,
    );

    Ok(())
}

// CSV MAPPING

pub fn match_mappings<'a>(mappings: &'a [Mapping], msg: &OscMessage) -> Vec<&'a Mapping> {
    let found_mappings: Vec<&'a Mapping> = mappings
        .iter()
        .filter(|m| {
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
        })
        .collect();

    if !found_mappings.is_empty() {
        for mapping in &found_mappings {
            let _ = custom_print(
                format!(
                    "Found mapping: {:?} {:?}",
                    mapping.osc_in_address,
                    mapping.osc_in_args.as_deref()
                ),
                Output::App,
            );
        }
    } else {
        println!("No mapping found for {:?}", msg.addr);
    }
    found_mappings
}

// HANDLE QC

fn parse_preset_id(preset_id: &str) -> Option<(u32, char)> {
    let re = Regex::new(r"^(\d+)([A-H])$").unwrap();
    if let Some(caps) = re.captures(preset_id) {
        let number = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let letter = caps.get(2).and_then(|m| m.as_str().chars().next());
        if let (Some(number), Some(letter)) = (number, letter) {
            Some((number, letter))
        } else {
            let _ = custom_print(
                format!(
                    "Invalid Quad Cortex preset format: {}. Expected format like '1A', '12D', etc.",
                    preset_id
                ),
                Output::App,
            );
            None
        }
    } else {
        let _ = custom_print(
            format!(
                "Invalid Quad Cortex preset format: {}. Expected format like '1A', '12D', etc.",
                preset_id
            ),
            Output::App,
        );
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

pub fn send_qc_preset(preset_id: &String, setlist: &u32, channel: &u32) -> Option<u32> {
    if let Some((number, letter)) = parse_preset_id(preset_id) {
        let program_change_number = parse_preset_midi(&number, &letter);
        let _ = custom_print(
            format!(
                "Sending QC Preset: Setlist {}, Preset {} -> PC: {} @ Ch: {}",
                setlist,
                preset_id,
                program_change_number.unwrap(),
                channel,
            ),
            Output::App,
        );
        return program_change_number;
    } else {
        // Handle the error case if needed
        return None;
    }
}
