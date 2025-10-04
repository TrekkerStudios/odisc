use crate::odisc::main::custom_print;
use crate::odisc::main::helpers::Mapping;
use crate::odisc::main::Output;
use regex::Regex;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use std::io;
use tokio::net::UdpSocket;
use smallvec::SmallVec;

use lazy_static::lazy_static;

lazy_static! {
    static ref QC_PRESET_REGEX: Regex = Regex::new(r"^(\d+)([A-H])$").unwrap();
    static ref GT1000_PRESET_REGEX: Regex = Regex::new(r"^(U|P)(\d{1,2})-(\d)$").unwrap();
}

// OSC

pub async fn incoming_osc_handler(sock: &UdpSocket, buf: &mut [u8]) -> io::Result<OscPacket> {
    let (len, _addr) = sock.recv_from(buf).await?;
    let (_rest, packet) = decoder::decode_udp(&buf[..len])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(packet)
}

pub async fn outgoing_osc_handler(
    sock: &UdpSocket,
    osc_out_address: &str,
    osc_out_args: Option<&str>,
    osc_host: &str,
    osc_port: &u16,
) -> std::io::Result<()> {
    // Stack-allocate for up to 8 args, heap for more
    let final_args: SmallVec<[OscType; 8]> = match osc_out_args {
        Some(args) if !args.trim().is_empty() => {
            args.split_whitespace()
                .filter_map(|arg| {
                    if let Ok(num) = arg.parse::<f32>() {
                        Some(OscType::Float(num))
                    } else if !arg.is_empty() {
                        Some(OscType::String(arg.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => SmallVec::new(),
    };

    let msg = OscMessage {
        addr: osc_out_address.to_string(),
        args: final_args.to_vec(), // Convert to Vec for OscMessage
    };
    let packet = OscPacket::Message(msg);
    let encoded = encoder::encode(&packet)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", e)))?;

    let final_port = if osc_out_address.contains("synth/fx") {
        osc_port + 1
    } else {
        *osc_port
    };
    
    let addr = format!("{osc_host}:{final_port}");
    sock.send_to(&encoded, addr).await?;
    
    // Remove custom_print entirely from this hot path
    Ok(())
}

// CSV MAPPING

pub fn match_mappings(mappings: &[Mapping], msg: &OscMessage) -> Vec<Mapping> {
    let found_mappings: Vec<Mapping> = mappings
        .iter()
        .filter(|m| {
            let addr_match = m.osc_in_address == msg.addr;

            let args_match = match &m.osc_in_args {
                None => true,
                Some(s) if s.is_empty() => true,
                Some(expected) => {
                    matches!(&msg.args[..], 
                        [rosc::OscType::String(val)] if val == expected)
                }
            };

            addr_match && args_match
        })
        .cloned()
        .collect();

    found_mappings
}

// HANDLE QC

fn parse_preset_id(preset_id: &str) -> Option<(u32, char)> {
    if let Some(caps) = QC_PRESET_REGEX.captures(preset_id) {
        let number = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let letter = caps.get(2).and_then(|m| m.as_str().chars().next());
        if let (Some(number), Some(letter)) = (number, letter) {
            Some((number, letter))
        } else {
            let _ = custom_print(
                format!(
                    "Invalid Quad Cortex preset format: {preset_id}. Expected format like '1A', '12D', etc."
                ),
                Output::App,
            );
            None
        }
    } else {
        let _ = custom_print(
            format!(
                "Invalid Quad Cortex preset format: {preset_id}. Expected format like '1A', '12D', etc."
            ),
            Output::App,
        );
        None
    }
}

fn parse_preset_midi(number: &u32, letter: &char) -> Option<u32> {
    if !(&1u32..=&32u32).contains(&number) {
        eprintln!("Invalid bank number: {number}. Must be between 1 and 32.");
        return None;
    }

    let preset_offset: u32 = (*letter as u32) - ('A' as u32);
    let pgm_ch_num = (number - 1) * 8 + preset_offset;

    Some(pgm_ch_num)
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
        program_change_number
    } else {
        // Handle the error case if needed
        None
    }
}

// HANDLE GT-1000

fn parse_gt1000_preset_id(preset_id: &str) -> Option<(char, u32, u32)> {
    if let Some(caps) = GT1000_PRESET_REGEX.captures(preset_id) {
        let preset_type = caps.get(1).and_then(|m| m.as_str().chars().next());
        let bank_number = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        let patch_number = caps.get(3).and_then(|m| m.as_str().parse::<u32>().ok());

        if let (Some(pt), Some(bn), Some(pn)) = (preset_type, bank_number, patch_number) {
            if (1..=50).contains(&bn) && (1..=5).contains(&pn) {
                return Some((pt, bn, pn));
            } else {
                let _ = custom_print(
                    format!(
                        "Invalid GT-1000 preset value: {preset_id}. Bank must be 1-50, patch 1-5."
                    ),
                    Output::App,
                );
                return None;
            }
        }
    }

    let _ = custom_print(
        format!(
            "Invalid GT-1000 preset format: {preset_id}. Expected format like 'U01-1' or 'P50-5'."
        ),
        Output::App,
    );
    None
}

fn parse_gt1000_preset_midi(
    preset_type: &char,
    bank_number: &u32,
    patch_number: &u32,
) -> Option<(u32, u32, u32)> {
    if !(1..=50).contains(bank_number) {
        eprintln!("Invalid bank number: {bank_number}. Must be between 1 and 50.");
        return None;
    }
    if !(1..=5).contains(patch_number) {
        eprintln!("Invalid patch number: {patch_number}. Must be between 1 and 5.");
        return None;
    }

    let bank_select_msb = 0;
    let bank_select_lsb = match preset_type {
        'U' => bank_number - 1,
        'P' => bank_number - 1 + 50,
        _ => {
            eprintln!("Invalid preset type: {preset_type}. Must be 'U' or 'P'.");
            return None;
        }
    };
    let program_change_number = patch_number - 1;

    Some((bank_select_msb, bank_select_lsb, program_change_number))
}

pub fn send_gt1000_preset(preset_id: &String, channel: &u32) -> Option<(u32, u32, u32)> {
    if let Some((preset_type, bank_number, patch_number)) = parse_gt1000_preset_id(preset_id) {
        if let Some((bank_select_msb, bank_select_lsb, program_change_number)) =
            parse_gt1000_preset_midi(&preset_type, &bank_number, &patch_number)
        {
            let _ = custom_print(
                format!(
                    "Sending GT-1000 Preset: {preset_id} -> Bank MSB: {bank_select_msb}, Bank LSB: {bank_select_lsb}, PC: {program_change_number} @ Ch: {channel}",
                ),
                Output::App,
            );
            return Some((bank_select_msb, bank_select_lsb, program_change_number));
        }
        None
    } else {
        // Error already printed in parse_gt1000_preset_id
        None
    }
}
