mod handlers;
mod helpers;
mod midi;
use midir::MidiOutput;
use rosc::OscPacket;
use tauri::AppHandle;
use tokio::net::UdpSocket;
use tokio::signal;

pub async fn backend(app_handle: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Check/create files
    let (mappings_path, config_path) = helpers::ensure_files()?;

    // Load mappings
    let mappings = match helpers::read_csv_file(mappings_path.to_str().unwrap()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading mappings: {}", e);
            return Err(e.into());
        }
    };

    // Initialize MIDI
    let midi_out = MidiOutput::new("MIDIOutput")?;
    let midi_outputs_list = midi::list_midi_devices(&midi_out);
    println!("Available MIDI devices:");
    for (i, name) in midi_outputs_list.iter().enumerate() {
        println!("{}: {}", i, name);
    }

    // Load config
    let config = helpers::read_config(config_path.to_str().unwrap(), midi_outputs_list)?;
    println!("{:?}", config);

    // Create OSC listener
    let addr = format!("0.0.0.0:{}", config.osc_listen_port);
    let sock = UdpSocket::bind(addr).await?;
    let mut buf = [0; 1024];

    // Connect to the chosen MIDI port
    let mut conn_out = match midi::connect_to_midi_port(midi_out, &config.midi_output_name, &app_handle) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Error connecting to MIDI port: {}", e);
            return Ok(());
        }
    };

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
                        if let Some(found_map) = handlers::match_mappings(&mappings, &msg, app_handle) {
                            // Handle outgoing OSC
                            handlers::outgoing_osc_handler(
                                &sock,
                                found_map.osc_out_address.as_ref().unwrap().as_str(),
                                found_map.osc_out_args.as_deref(),
                                &config.osc_send_host,
                                &config.osc_send_port,
                                app_handle
                            )
                            .await?;

                            // Handle MIDI message
                            if let Err(e) = midi::handle_midi_message(&mut conn_out, &found_map, &app_handle) {
                                eprintln!("Error sending MIDI message: {}", e);
                            }
                        } else {
                            println!("Mapping not found.");
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
