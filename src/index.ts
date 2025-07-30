import { Server as OscServer, Client as OscClient, Message } from 'node-osc';
import { Output as MidiOutput, getOutputs } from 'easymidi';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import csv from 'csv-parser';

const printMappings = process.argv.includes('--print-mappings');

// --- Directory Setup ---
const odiscDir = path.join(os.homedir(), 'Documents', 'odisc');
if (!fs.existsSync(odiscDir)) {
  fs.mkdirSync(odiscDir, { recursive: true });
}

const configPath = path.join(odiscDir, 'config.json');
const defaultMappingsPath = path.join(odiscDir, 'mappings.csv');

// --- Ensure mappings.csv exists (column headers only) ---
const mappingsHeaders = [
  'osc_in_address',
  'osc_in_args',
  'osc_out_address',
  'osc_out_args',
  'midi_channel',
  'midi_type',
  'midi_note',
  'midi_velocity',
  'midi_controller',
  'midi_value',
  'setlist',
  'qc_preset_id'
].join(',') + '\n';

if (!fs.existsSync(defaultMappingsPath)) {
  console.warn(`mappings.csv not found. Creating default mappings.csv at ${defaultMappingsPath}`);
  fs.writeFileSync(defaultMappingsPath, mappingsHeaders, 'utf8');
}

// --- MIDI Device Detection ---
const midiOutputs = getOutputs();
let selectedMidiDevice = midiOutputs.length > 0 ? midiOutputs[0] : "";

// --- Ensure config.json exists ---
if (!fs.existsSync(configPath)) {
  console.warn(`config.json not found. Creating default config.json at ${configPath}`);
  const defaultConfig = {
    OSC_LISTEN_PORT: 8000,
    OSC_SEND_HOST: "127.0.0.1",
    OSC_SEND_PORT: 7001,
    MIDI_OUTPUT_NAME: selectedMidiDevice
  };
  fs.writeFileSync(configPath, JSON.stringify(defaultConfig, null, 2), 'utf8');
}

// --- Load and possibly update config.json ---
let config: any;
try {
  const rawConfig = fs.readFileSync(configPath, 'utf8');
  config = JSON.parse(rawConfig);

  // If MIDI_OUTPUT_NAME is missing or not available, set to first available and update config.json
  if (!config.MIDI_OUTPUT_NAME || !midiOutputs.includes(config.MIDI_OUTPUT_NAME)) {
    if (midiOutputs.length > 0) {
      config.MIDI_OUTPUT_NAME = midiOutputs[0];
      console.warn(`Configured MIDI device not found. Setting MIDI_OUTPUT_NAME to first available: "${midiOutputs[0]}"`);
    } else {
      config.MIDI_OUTPUT_NAME = "";
      console.warn("No MIDI output devices available. MIDI output will be disabled.");
    }
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2), 'utf8');
  }
} catch (error) {
  console.error('Error reading or parsing config.json:', error);
  process.exit(1);
}

const { OSC_LISTEN_PORT, OSC_SEND_HOST, OSC_SEND_PORT, MIDI_OUTPUT_NAME } = config;

// --- Interfaces ---
interface Mapping {
  osc_in_address: string;
  osc_in_args?: string;
  osc_out_address?: string;
  osc_out_args?: string;
  midi_channel?: number;
  midi_type?: 'note_on' | 'note_off' | 'cc' | 'pc' | 'qc_preset';
  midi_note?: number;
  midi_velocity?: number;
  midi_controller?: number;
  midi_value?: number;
  qc_preset_id?: string;
  setlist?: number;
}

// --- State ---
let mappings: Mapping[] = [];
let currentSetlist: number = 0;

// --- Functions ---

const sendQuadCortexPreset = (presetId: string, channel: number, midiOut: MidiOutput, setlist: number) => {
  const match = presetId.match(/^(\d+)([A-H])$/i);
  if (!match || !match[1] || !match[2]) {
    console.error(`Invalid Quad Cortex preset format: ${presetId}. Expected format like '1A', '12D', etc.`);
    return;
  }

  const bank = parseInt(match[1], 10);
  const presetChar = match[2].toUpperCase();

  if (bank < 1 || bank > 32) {
    console.error(`Invalid bank number: ${bank}. Must be between 1 and 32.`);
    return;
  }

  const presetOffset = presetChar.charCodeAt(0) - 'A'.charCodeAt(0);
  const programChangeNumber = (bank - 1) * 8 + presetOffset;

  console.log(`Sending QC Preset: Setlist ${setlist}, Preset ${presetId} -> PC: ${programChangeNumber}`);

  // @ts-ignore
  midiOut.send('cc', { controller: 0, value: 0, channel: channel });
  // @ts-ignore
  midiOut.send('cc', { controller: 32, value: setlist, channel: channel });
  // @ts-ignore
  midiOut.send('program', { number: programChangeNumber, channel: channel });

  console.log(`Sent MIDI for QC Preset: CC#0 val 0, CC#32 val ${setlist}, PC# ${programChangeNumber} on channel ${channel}`);
};

const loadMappings = (filePath: string): Promise<Mapping[]> => {
  return new Promise((resolve, reject) => {
    const results: Mapping[] = [];
    fs.createReadStream(filePath)
      .pipe(csv())
      .on('data', (data) => {
        const mapping: any = {};
        for (const key in data) {
          const value = data[key];
          if (value !== '' && !isNaN(Number(value))) {
            mapping[key] = Number(value);
          } else {
            mapping[key] = value === '' ? undefined : value;
          }
        }
        results.push(mapping as Mapping);
      })
      .on('end', () => {
        if (printMappings) {
          console.log('Mappings loaded successfully:');
          console.table(results);
        }
        resolve(results);
      })
      .on('error', (error) => {
        reject(error);
      });
  });
};

const handleOscMessage = (msg: any, oscClient: OscClient, midiOut: MidiOutput | null) => {
  const [address, ...args] = msg;
  const normalizedAddress = address.endsWith('/') ? address.slice(0, -1) : address;
  console.log(`Received OSC message: ${address} (normalized to ${normalizedAddress})`, args);

  if (normalizedAddress === '/setlist') {
    if (typeof args[0] === 'number') {
      currentSetlist = args[0];
      console.log(`Current Setlist updated to: ${currentSetlist}`);
    } else {
      console.warn(`Invalid argument for /setlist. Expected a number.`);
    }
    return;
  }

  const mapping = mappings.find(m => {
    if (m.osc_in_address !== normalizedAddress) {
      return false;
    }
    if (m.osc_in_args) {
      const expectedArgs = String(m.osc_in_args).split(' ');
      const receivedArgs = args.map(String);
      if (expectedArgs.length !== receivedArgs.length) {
        return false;
      }
      return expectedArgs.every((expected, i) => expected === receivedArgs[i]);
    }
    return true;
  });

  if (!mapping) {
    console.log(`No mapping found for address: ${normalizedAddress} with args: ${JSON.stringify(args)}`);
    return;
  }

  // --- OSC Output ---
  if (mapping.osc_out_address) {
    let finalArgs: any[] = [];
    if (mapping.osc_out_args != null && String(mapping.osc_out_args).trim() !== '') {
      const argsString = String(mapping.osc_out_args);
      finalArgs = argsString.split(' ').map(arg => {
        const num = parseFloat(arg);
        return isNaN(num) ? arg : { type: 'f', value: num };
      });
    } else {
      finalArgs = args.map((arg: any) => {
        if (typeof arg === 'number') {
          return { type: 'f', value: arg };
        }
        return arg;
      });
    }
    const oscMessage = new Message(mapping.osc_out_address);
    finalArgs.forEach(arg => oscMessage.append(arg));
    oscClient.send(oscMessage, (err) => {
      if (err) console.error(`Error sending OSC message:`, err);
      else {
        const loggedArgs = finalArgs.map(a => (a && a.value) !== undefined ? a.value : a);
        console.log(`Sent OSC message to ${mapping.osc_out_address}:`, loggedArgs);
      }
    });
  }

  // --- MIDI Output ---
  if (midiOut && mapping.midi_type && mapping.midi_channel !== undefined) {
    const channel = mapping.midi_channel;
    switch (mapping.midi_type) {
      case 'note_on':
        if (mapping.midi_note !== undefined && mapping.midi_velocity !== undefined) {
          // @ts-ignore
          midiOut.send('noteon', {
            note: mapping.midi_note,
            velocity: mapping.midi_velocity,
            channel: channel,
          });
          console.log(`Sent MIDI noteon: ch=${channel}, note=${mapping.midi_note}, vel=${mapping.midi_velocity}`);
        }
        break;
      case 'note_off':
        if (mapping.midi_note !== undefined && mapping.midi_velocity !== undefined) {
          // @ts-ignore
          midiOut.send('noteoff', {
            note: mapping.midi_note,
            velocity: mapping.midi_velocity,
            channel: channel,
          });
          console.log(`Sent MIDI noteoff: ch=${channel}, note=${mapping.midi_note}, vel=${mapping.midi_velocity}`);
        }
        break;
      case 'cc':
        if (mapping.midi_controller !== undefined) {
          const value = mapping.midi_value !== undefined ? mapping.midi_value : (typeof args[0] === 'number' ? args[0] : 0);
          // @ts-ignore
          midiOut.send('cc', {
            controller: mapping.midi_controller,
            value: value,
            channel: channel,
          });
          console.log(`Sent MIDI CC: ch=${channel}, controller=${mapping.midi_controller}, value=${value}`);
        }
        break;
      case 'pc':
        const value = mapping.midi_value !== undefined ? mapping.midi_value : (typeof args[0] === 'number' ? args[0] : 0);
        // @ts-ignore
        midiOut.send('program', {
          number: value,
          channel: channel,
        });
        console.log(`Sent MIDI Program Change: ch=${channel}, program=${value}`);
        break;
      case 'qc_preset':
        if (mapping.qc_preset_id) {
          const setlist = mapping.setlist !== undefined ? mapping.setlist : currentSetlist;
          sendQuadCortexPreset(mapping.qc_preset_id, channel, midiOut, setlist);
        }
        break;
    }
  }
};

// --- Main Application Logic ---
const main = async () => {
  try {
    mappings = await loadMappings(defaultMappingsPath);

    // --- Initialize OSC Server ---
    const oscServer = new OscServer(OSC_LISTEN_PORT, '0.0.0.0', () => {
      console.log(`OSC Server is listening on port ${OSC_LISTEN_PORT}`);
    });

    // --- Initialize OSC Client ---
    const oscClient = new OscClient(OSC_SEND_HOST, OSC_SEND_PORT);
    console.log(`OSC Client will send to ${OSC_SEND_HOST}:${OSC_SEND_PORT}`);

    // --- Initialize MIDI Output ---
    let midiOut: MidiOutput | null = null;
    try {
      console.log('Available MIDI outputs:', midiOutputs);
      if (MIDI_OUTPUT_NAME && midiOutputs.includes(MIDI_OUTPUT_NAME)) {
        midiOut = new MidiOutput(MIDI_OUTPUT_NAME);
        console.log(`MIDI Output is set to "${MIDI_OUTPUT_NAME}"`);
      } else if (midiOutputs.length > 0) {
        midiOut = new MidiOutput(midiOutputs[0]);
        console.log(`MIDI Output is set to first available: "${midiOutputs[0]}"`);
      } else {
        console.warn("No MIDI output devices available. MIDI output will be disabled.");
      }
    } catch (error) {
      console.error('Could not initialize MIDI output:', error);
    }

    // --- Set up OSC Message Handler ---
    oscServer.on('message', (msg: any) => {
      handleOscMessage(msg, oscClient, midiOut);
    });

    // --- Graceful Shutdown ---
    process.on('SIGINT', () => {
      console.log('Closing OSC server and MIDI output.');
      oscServer.close();
      if (midiOut) midiOut.close();
      process.exit(0);
    });

  } catch (error) {
    if ((error as any).code === 'ENOENT') {
      console.error(`Error: Could not find the mapping file at '${defaultMappingsPath}'. Please create it.`);
    } else {
      console.error('Failed to start the application:', error);
    }
    process.exit(1);
  }
};

main();