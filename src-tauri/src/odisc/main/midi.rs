use crate::odisc::main::helpers::Mapping;
use crate::odisc::main::{custom_print, handlers, Output};
use midir::{MidiOutput, MidiOutputConnection};
use std::error::Error;

pub fn list_midi_devices(midi_out: &MidiOutput) -> Vec<String> {
    midi_out
        .ports()
        .iter()
        .map(|p| midi_out.port_name(p).unwrap_or_default())
        .collect()
}

pub fn connect_to_midi_port(
    midi_out: MidiOutput,
    port_name_to_find: &str,
) -> Result<MidiOutputConnection, Box<dyn Error>> {
    let out_ports = midi_out.ports();
    let port = out_ports.iter().find(|p| {
        midi_out
            .port_name(p)
            .is_ok_and(|name| name == port_name_to_find)
    });

    match port {
        Some(port) => {
            let port_name = midi_out.port_name(port)?;
            let conn = midi_out.connect(port, "midir-connection")?;
            println!("Successfully connected to MIDI output port: {port_name}");
            Ok(conn)
        }
        None => {
            let _ = custom_print(
                format!("No port found with name '{port_name_to_find}'"),
                Output::AppError,
            );
            Err(format!("No port found with name '{port_name_to_find}'").into())
        }
    }
}

pub fn handle_midi_message(
    conn_out: &mut MidiOutputConnection,
    found_map: &Mapping,
) -> Result<(), Box<dyn Error>> {
    match found_map.midi_type.as_deref() {
        Some("note_on") => {
            if let (Some(note), Some(velocity), Some(channel)) = (
                found_map.midi_note,
                found_map.midi_velocity,
                found_map.midi_channel,
            ) {
                let channel = (channel as u8).saturating_sub(1); // 0-based
                let msg = [0x90 | channel, note as u8, velocity as u8];
                conn_out.send(&msg)?;
                let _ = custom_print(
                    format!(
                        "Sent MIDI note_on: ch={}, note={}, vel={}",
                        channel + 1,
                        note,
                        velocity
                    ),
                    Output::App,
                );
            }
        }
        Some("note_off") => {
            if let (Some(note), Some(velocity), Some(channel)) = (
                found_map.midi_note,
                found_map.midi_velocity,
                found_map.midi_channel,
            ) {
                let channel = (channel as u8).saturating_sub(1);
                let msg = [0x80 | channel, note as u8, velocity as u8];
                conn_out.send(&msg)?;
                let _ = custom_print(
                    format!(
                        "Sent MIDI note_off: ch={}, note={}, vel={}",
                        channel + 1,
                        note,
                        velocity
                    ),
                    Output::App,
                );
            }
        }
        Some("cc") => {
            if let (Some(controller), Some(channel)) =
                (found_map.midi_controller, found_map.midi_channel)
            {
                // Value: from mapping, or fallback to 0
                let value = found_map.midi_value.unwrap_or(0);
                let channel = (channel as u8).saturating_sub(1);
                let msg = [0xB0 | channel, controller as u8, value as u8];
                conn_out.send(&msg)?;
                let _ = custom_print(
                    format!(
                        "Sent MIDI CC: ch={}, controller={}, value={}",
                        channel + 1,
                        controller,
                        value
                    ),
                    Output::App,
                );
            }
        }
        Some("pc") => {
            if let Some(channel) = found_map.midi_channel {
                let value = found_map.midi_value.unwrap_or(0);
                let channel = (channel as u8).saturating_sub(1);
                let msg = [0xC0 | channel, value as u8];
                conn_out.send(&msg)?;
                let _ = custom_print(
                    format!(
                        "Sent MIDI Program Change: ch={}, program={}",
                        channel + 1,
                        value
                    ),
                    Output::App,
                );
            }
        }
        Some("qc_preset") => {
            let pgm = handlers::send_qc_preset(
                found_map.qc_preset_id.as_ref().unwrap(),
                &found_map.setlist.unwrap(),
                &found_map.midi_channel.unwrap(),
            );

            let channel = found_map.midi_channel.unwrap() as u8 - 1;
            let setlist: u8 = found_map.setlist.unwrap() as u8;
            // Send CC#0 value 0
            let cc0_msg = [0xB0 | channel, 0, 0];
            conn_out.send(&cc0_msg)?;
            // Send CC#32 value setlist
            let cc32_msg = [0xB0 | channel, 32, setlist];
            conn_out.send(&cc32_msg)?;
            // Send PC value from parser
            let program = pgm.unwrap() as u8;
            let pc_msg = [0xC0 | channel, program];
            conn_out.send(&pc_msg)?;
        }
        Some("gt1000_preset") => {
            if let (Some(preset_id), Some(channel)) = (
                found_map.gt1000_preset_id.as_ref(),
                found_map.midi_channel,
            ) {
                if let Some((bank_msb, bank_lsb, pgm)) = handlers::send_gt1000_preset(preset_id, &channel) {
                    let channel = (channel as u8).saturating_sub(1);
                    // Bank Select MSB
                    let cc0_msg = [0xB0 | channel, 0, bank_msb as u8];
                    conn_out.send(&cc0_msg)?;
                    // Bank Select LSB
                    let cc32_msg = [0xB0 | channel, 32, bank_lsb as u8];
                    conn_out.send(&cc32_msg)?;
                    // Program Change
                    let msg = [0xC0 | channel, pgm as u8];
                    conn_out.send(&msg)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
