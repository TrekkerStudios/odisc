import { Server as OscServer, Client as OscClient, Message } from 'node-osc';
import { Output as MidiOutput, getOutputs } from 'easymidi';
import * as fs from 'fs';
import csv from 'csv-parser';

// --- Configuration Loading ---
let config: any;
try {
    const rawConfig = fs.readFileSync('./config.json', 'utf8');
    config = JSON.parse(rawConfig);
} catch (error) {
    console.error('Error reading or parsing config.json:', error);
    process.exit(1);
}

const { OSC_LISTEN_PORT, OSC_SEND_HOST, OSC_SEND_PORT, MIDI_OUTPUT_NAME, MAPPINGS_CSV_PATH } = config;

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

/**
 * Sends the MIDI messages to select a preset on the Quad Cortex.
 * @param presetId The preset identifier (e.g., '1A', '32H').
 * @param channel The MIDI channel to send on.
 * @param midiOut The MIDI output instance.
 * @param setlist The setlist number to use.
 */
const sendQuadCortexPreset = (presetId: string, channel: number, midiOut: MidiOutput, setlist: number) => {
    const match = presetId.match(/^(\d+)([A-H])$/i);
    if (!match) {
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

    // 1. Send Bank Select MSB (CC 0, value 0 for presets)
    midiOut.send('cc', { controller: 0, value: 0, channel: channel });
    // 2. Send Bank Select LSB (CC 32, value is the setlist number)
    midiOut.send('cc', { controller: 32, value: setlist, channel: channel });
    // 3. Send Program Change
    midiOut.send('program', { number: programChangeNumber, channel: channel });

    console.log(`Sent MIDI for QC Preset: CC#0 val 0, CC#32 val ${setlist}, PC# ${programChangeNumber} on channel ${channel}`);
};


/**
 * Loads and parses the mappings from the CSV file.
 * @param filePath Path to the CSV file.
 * @returns A promise that resolves with an array of mappings.
 */
const loadMappings = (filePath: string): Promise<Mapping[]> => {
  return new Promise((resolve, reject) => {
    const results: Mapping[] = [];
    fs.createReadStream(filePath)
      .pipe(csv({ trim: true }))
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
        console.log('Mappings loaded successfully:');
        console.table(results);
        resolve(results);
      })
      .on('error', (error) => {
        reject(error);
      });
  });
};

/**
 * Handles incoming OSC messages.
 * @param msg The OSC message.
 * @param oscClient The OSC client to send messages with.
 * @param midiOut The MIDI output to send messages with.
 */
const handleOscMessage = (msg: Message, oscClient: OscClient, midiOut: MidiOutput | null) => {
  const [address, ...args] = msg;
  // Normalize the address by removing any trailing slash to make matching more robust.
  const normalizedAddress = address.endsWith('/') ? address.slice(0, -1) : address;
  console.log(`Received OSC message: ${address} (normalized to ${normalizedAddress})`, args);

    // Special handler for setting the setlist
    if (normalizedAddress === '/setlist') {
        if (typeof args[0] === 'number') {
            currentSetlist = args[0];
            console.log(`Current Setlist updated to: ${currentSetlist}`);
        } else {
            console.warn(`Invalid argument for /setlist. Expected a number.`);
        }
        return; // Stop further processing
    }

  const mapping = mappings.find(m => {
    if (m.osc_in_address !== normalizedAddress) {
        return false;
    }

    // If osc_in_args is defined, we must match the arguments.
    if (m.osc_in_args) {
        const expectedArgs = String(m.osc_in_args).split(' ');
        const receivedArgs = args.map(String);

        if (expectedArgs.length !== receivedArgs.length) {
            return false;
        }

        return expectedArgs.every((expected, i) => expected === receivedArgs[i]);
    }

    // If osc_in_args is not defined, match any message with this address.
    return true;
  });

  if (!mapping) {
    console.log(`No mapping found for address: ${normalizedAddress} with args: ${JSON.stringify(args)}`);
    return;
  }

  // --- OSC Output ---
  if (mapping.osc_out_address) {
    let finalArgs: any[] = [];

    // Prioritize osc_out_args if it is defined (not null/undefined) and not an empty string.
    if (mapping.osc_out_args != null && String(mapping.osc_out_args).trim() !== '') {
        const argsString = String(mapping.osc_out_args);
        finalArgs = argsString.split(' ').map(arg => {
            const num = parseFloat(arg);
            return isNaN(num) ? arg : { type: 'f', value: num };
        });
    } else {
        // Otherwise, use the arguments from the incoming message.
        finalArgs = args.map(arg => {
            if (typeof arg === 'number') {
                return { type: 'f', value: arg };
            }
            return arg;
        });
    }

    // Explicitly construct the OSC message to ensure compatibility.
    const oscMessage = new Message(mapping.osc_out_address);
    finalArgs.forEach(arg => oscMessage.append(arg));

    oscClient.send(oscMessage, (err) => {
        if (err) console.error(`Error sending OSC message:`, err);
        else {
            // Log the sent values cleanly for readability
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
          // Use the first argument from the OSC message as the CC value if midi_value is not in the CSV
          const value = mapping.midi_value !== undefined ? mapping.midi_value : (typeof args[0] === 'number' ? args[0] : 0);
          midiOut.send('cc', {
            controller: mapping.midi_controller,
            value: value,
            channel: channel,
          });
          console.log(`Sent MIDI CC: ch=${channel}, controller=${mapping.midi_controller}, value=${value}`);
        }
        break;
      case 'pc':
        // Use the first argument from the OSC message as the program change value if midi_value is not in the CSV
        const value = mapping.midi_value !== undefined ? mapping.midi_value : (typeof args[0] === 'number' ? args[0] : 0);
        midiOut.send('program', {
            number: value,
            channel: channel,
        });
        console.log(`Sent MIDI Program Change: ch=${channel}, program=${value}`);
        break;
      case 'qc_preset':
        if (mapping.qc_preset_id) {
            // Use the setlist from the CSV if it exists, otherwise use the global currentSetlist
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
    // --- Ensure config.json exists and load it ---
    const configPath = './config.json';
    if (!fs.existsSync(configPath)) {
      console.warn(`config.json not found. Creating default config.json at ${configPath}`);
      const defaultConfigContent = JSON.stringify({
        OSC_LISTEN_PORT: 8000,
        OSC_SEND_HOST: "127.0.0.1",
        OSC_SEND_PORT: 7001,
        MIDI_OUTPUT_NAME: "IAC Driver Bus 1",
        MAPPINGS_CSV_PATH: "./mappings.csv"
      }, null, 2);
      fs.writeFileSync(configPath, defaultConfigContent, 'utf8');
    }
    const rawConfig = fs.readFileSync(configPath, 'utf8');
    config = JSON.parse(rawConfig);
    const { OSC_LISTEN_PORT, OSC_SEND_HOST, OSC_SEND_PORT, MIDI_OUTPUT_NAME, MAPPINGS_CSV_PATH } = config;

    // --- Ensure mappings.csv exists and load it ---
    if (!fs.existsSync(MAPPINGS_CSV_PATH)) {
      console.warn(`mappings.csv not found. Creating default mappings.csv at ${MAPPINGS_CSV_PATH}`);
      const defaultMappingsContent = "osc_in_address,osc_in_args,osc_out_address,osc_out_args,midi_channel,midi_type,midi_note,midi_velocity,midi_controller,midi_value,qc_preset_id,setlist\n/example/osc/in,arg1,/example/osc/out,outarg1,1,note_on,60,100,,,,";
      fs.writeFileSync(MAPPings_CSV_PATH, defaultMappingsContent, 'utf8');
    }
    mappings = await loadMappings(MAPPINGS_CSV_PATH);

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
        console.log('Available MIDI outputs:', getOutputs());
        if (getOutputs().includes(MIDI_OUTPUT_NAME)) {
            midiOut = new MidiOutput(MIDI_OUTPUT_NAME);
            console.log(`MIDI Output is set to "${MIDI_OUTPUT_NAME}"`);
        } else {
            console.warn(`MIDI output port "${MIDI_OUTPUT_NAME}" not found. MIDI output will be disabled.`);
            console.warn(`Please create a virtual MIDI port named "${MIDI_OUTPUT_NAME}" or change the name in the script.`);
        }
    } catch (error) {
        console.error('Could not initialize MIDI output:', error);
    }


    // --- Set up OSC Message Handler ---
    oscServer.on('message', (msg) => {
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
    if (error.code === 'ENOENT') {
        console.error(`Error: Could not find the mapping file at '${MAPPINGS_CSV_PATH}'. Please create it.`);
    } else {
        console.error('Failed to start the application:', error);
    }
    process.exit(1);
  }
};

main();
