use csv::Reader;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Mapping {
    pub osc_in_address: String,
    pub osc_in_args: Option<String>,
    pub osc_out_address: Option<String>,
    pub osc_out_args: Option<String>,
    pub midi_channel: Option<u32>,
    pub midi_type: Option<String>,
    pub midi_note: Option<u32>,
    pub midi_velocity: Option<u32>,
    pub midi_controller: Option<u32>,
    pub midi_value: Option<u32>,
    pub qc_preset_id: Option<String>,
    pub gt1000_preset_id: Option<String>,
    pub setlist: Option<u32>,
    pub _comment: Option<String>, // just for user reference, not actually used
}

pub fn load_mappings_from_csv(path: PathBuf) -> Result<Vec<Mapping>, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut rdr = Reader::from_reader(file);
    let mut mappings = Vec::new();

    for result in rdr.deserialize() {
        let record: Mapping = result?;
        mappings.push(record);
    }
    Ok(mappings)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Config {
    pub osc_listen_port: u16,
    pub osc_send_host: String,
    pub osc_send_port: u16,
    pub midi_output_name: String,
    #[serde(default)]
    pub debug_logging: bool,
}

pub fn read_config(
    path: &str,
    midi_outputs: Vec<String>,
) -> Result<Config, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader)?;

    if !midi_outputs.contains(&config.midi_output_name) {
        let mut new_config = config.clone();
        if let Some(first) = midi_outputs.first() {
            println!("Configured MIDI device not found. Setting to first available: {first}");
            new_config.midi_output_name = first.clone();
        } else {
            println!("No MIDI output devices available. MIDI output will be disabled.");
            new_config.midi_output_name = "".to_string();
        }
        std::fs::write(path, serde_json::to_string_pretty(&new_config).unwrap())?;
        return Ok(new_config);
    }

    Ok(config)
}

pub fn ensure_files() -> std::io::Result<(PathBuf, PathBuf)> {
    let home = dirs::home_dir().expect("Could not find home directory");
    let odisc_dir = home.join("Documents").join("odisc");
    if !odisc_dir.exists() {
        fs::create_dir_all(&odisc_dir)?;
        println!("Created directory: {odisc_dir:?}");
    }

    let mappings_path = odisc_dir.join("mappings.csv");
    if !mappings_path.exists() {
        let headers = "osc_in_address,osc_in_args,osc_out_address,osc_out_args,midi_channel,midi_type,midi_note,midi_velocity,midi_controller,midi_value,setlist,qc_preset_id,gt1000_preset_id\ncomment\n";
        fs::write(&mappings_path, headers)?;
        println!("Created default mappings.csv at {mappings_path:?}");
    }

    // Load mappings to confirm file is valid
    load_mappings_from_csv(mappings_path.clone()).expect("Failed to load mappings");

    let config_path = odisc_dir.join("config.json");
    if !config_path.exists() {
        let default_config = r#"{
  "OSC_LISTEN_PORT": 8000,
  "OSC_SEND_HOST": "127.0.0.1",
  "OSC_SEND_PORT": 7001,
  "MIDI_OUTPUT_NAME": "",
  "DEBUG_LOGGING": false
}"#;
        let mut file = fs::File::create(&config_path)?;
        file.write_all(default_config.as_bytes())?;
        println!("Created default config.json at {config_path:?}");
    }

    Ok((mappings_path, config_path))
}
