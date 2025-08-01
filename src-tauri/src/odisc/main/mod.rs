mod handlers;
mod helpers;
mod midi;
use crate::get_app_handle;
use midir::MidiOutput;
use rosc::OscPacket;
use tauri::{AppHandle, Emitter};
use tokio::net::UdpSocket;
use tokio::signal;
use serde_json::json;

pub enum Output {
    Console,
    // Error,
    App,
    AppError,
}

pub fn custom_print(msg: String, type_output: Output) -> Result<(), Box<dyn std::error::Error>> {
    match type_output {
        Output::Console => {
            println!("{}", msg);
            Ok(())
        }
        // Output::Error => {
        //     eprintln!("{}", msg);
        //     Ok(())
        // }
        Output::App => {
            if let Some(app_handle) = get_app_handle() {
                app_handle.emit("backend-log", format!("📥 {}", &msg))?;
                println!("{}", msg);
            } else {
                eprintln!("App handle not set!");
            }
            Ok(())
        }
        Output::AppError => {
            if let Some(app_handle) = get_app_handle() {
                app_handle.emit("backend-log", format!("❌ {}", &msg))?;
                eprintln!("{}", msg);
            } else {
                eprintln!("App handle not set!");
            }
            Ok(())
        }
    }
}

pub async fn backend(app_handle: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Check/create files
    let (mappings_path, config_path) = helpers::ensure_files()?;

    // Load mappings
    let mappings = match helpers::read_csv_file(mappings_path.to_str().unwrap()) {
        Ok(m) => m,
        Err(e) => {
            let _ = custom_print(format!("Error loading mappings: {}", e), Output::App);
            return Err(e.into());
        }
    };
    let _ = custom_print(format!("Mappings loaded!"), Output::App);

    // Initialize MIDI
    let midi_out = MidiOutput::new("MIDIOutput")?;
    let midi_outputs_list = midi::list_midi_devices(&midi_out);
    let _ = custom_print(format!("Available MIDI devices:"), Output::App);
    for (i, name) in midi_outputs_list.iter().enumerate() {
        let _ = custom_print(format!("{}: {}", i, name), Output::App);
    }

    // Load config
    let config = helpers::read_config(config_path.to_str().unwrap(), midi_outputs_list)?;
    println!("{:?}", config);

    // Create OSC listener
    let addr = format!("0.0.0.0:{}", config.osc_listen_port);
    let sock = UdpSocket::bind(addr).await?;
    let mut buf = [0; 1024];
    let _ = custom_print(
        format!("OSC server listening on port {}", &config.osc_listen_port),
        Output::App,
    );
    let _ = custom_print(
        format!(
            "OSC server sending on {}:{}",
            &config.osc_send_host, &config.osc_send_port
        ),
        Output::App,
    );

    // Connect to the chosen MIDI port
    let mut conn_out = match midi::connect_to_midi_port(midi_out, &config.midi_output_name) {
        Ok(conn) => conn,
        Err(e) => {
            let _ = custom_print(format!("Error connecting to MIDI port: {}", e), Output::App);
            return Ok(());
        }
    };
    let _ = custom_print(
        format!("MIDI device connected: {}", &config.midi_output_name),
        Output::App,
    );

    let network_payload = json!({
        "osc_listen_port": config.osc_listen_port.to_string(),
        "osc_send_port": config.osc_send_port.to_string(),
        "osc_send_host": config.osc_send_host,
    });
    app_handle.emit("network-data", network_payload.to_string())?;

    println!("Application started. Press Ctrl+C to exit.");
    // Listen for OSC packets
    loop {
        tokio::select! {
            packet_result = handlers::incoming_osc_handler(&sock, &mut buf) => {
                let packet = packet_result?;
                match packet {
                    OscPacket::Message(msg) => {
                        println!("Address: {}", msg.addr);
                        println!("Arguments: {:?}", msg.args);
                        let found_maps = handlers::match_mappings(&mappings, &msg);
                        if !found_maps.is_empty() {
                            for found_map in found_maps {
                                // Handle outgoing OSC
                                if let Some(addr) = &found_map.osc_out_address {
                                    if !addr.is_empty() {
                                        handlers::outgoing_osc_handler(
                                            &sock,
                                            addr.as_str(),
                                            found_map.osc_out_args.as_deref(),
                                            &config.osc_send_host,
                                            &config.osc_send_port,
                                        )
                                        .await?;
                                    }
                                }

                                // Handle MIDI message
                                if let Err(e) =
                                    midi::handle_midi_message(&mut conn_out, &found_map)
                                {
                                    let _ = custom_print(
                                        format!("Error sending MIDI message: {}", e),
                                        Output::App,
                                    );
                                }
                            }
                        } else {
                            let _ = custom_print(format!("Mapping not found."), Output::Console);
                        }
                    }
                    _ => {
                        println!("Received a non-message packet");
                    }
                }
            },
            _ = signal::ctrl_c() => {
                break;
            }
        }
    }

    println!("Exiting main loop. Cleaning up...");
    Ok(())
}
