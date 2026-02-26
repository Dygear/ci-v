use wasm_bindgen::prelude::*;

use civ_protocol::command::{cmd, meter_sub, tone_sub, various_sub, Command};
use civ_protocol::frequency::Frequency;
use civ_protocol::mode::OperatingMode;
use civ_protocol::protocol::{Frame, PREAMBLE};
use civ_protocol::response::{self, Response};

/// Accumulates raw bytes from WebSerial and extracts complete CI-V frames.
#[wasm_bindgen]
pub struct FrameBuffer {
    buf: Vec<u8>,
}

#[wasm_bindgen]
impl FrameBuffer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(256),
        }
    }

    /// Feed raw bytes from WebSerial into the buffer.
    /// Returns an array of parsed response objects (may be empty if no complete frames yet).
    pub fn feed(&mut self, data: &[u8]) -> Result<JsValue, JsValue> {
        self.buf.extend_from_slice(data);

        let responses = js_sys::Array::new();

        loop {
            match Frame::parse(&self.buf) {
                Ok(Some((frame, consumed))) => {
                    // Find where the frame started in the buffer
                    let start = self
                        .buf
                        .windows(2)
                        .position(|w| w[0] == PREAMBLE && w[1] == PREAMBLE)
                        .unwrap_or(0);
                    self.buf.drain(..start + consumed);

                    // Skip echo frames (frames we sent — dst is the radio, src is us)
                    if frame.src == civ_protocol::protocol::ADDR_CONTROLLER {
                        continue;
                    }

                    let js_response = self.frame_to_js(&frame)?;
                    responses.push(&js_response);
                }
                Ok(None) => break, // incomplete frame, wait for more data
                Err(_) => {
                    // Invalid frame data — discard up to the next preamble
                    if let Some(pos) = self
                        .buf
                        .windows(2)
                        .skip(1)
                        .position(|w| w[0] == PREAMBLE && w[1] == PREAMBLE)
                    {
                        self.buf.drain(..pos + 1);
                    } else {
                        self.buf.clear();
                    }
                    break;
                }
            }
        }

        Ok(responses.into())
    }

    /// Clear the internal buffer.
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Returns the number of buffered bytes.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    fn frame_to_js(&self, frame: &Frame) -> Result<JsValue, JsValue> {
        let resp = self.infer_response(frame)?;

        let obj = js_sys::Object::new();
        match resp {
            Response::Ok => {
                js_sys::Reflect::set(&obj, &"type".into(), &"ok".into())?;
            }
            Response::Ng => {
                js_sys::Reflect::set(&obj, &"type".into(), &"ng".into())?;
            }
            Response::Frequency(freq) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"frequency".into())?;
                js_sys::Reflect::set(&obj, &"hz".into(), &JsValue::from_f64(freq.hz() as f64))?;
                js_sys::Reflect::set(
                    &obj,
                    &"display".into(),
                    &JsValue::from_str(&freq.to_string()),
                )?;
            }
            Response::Mode(mode) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"mode".into())?;
                js_sys::Reflect::set(
                    &obj,
                    &"mode".into(),
                    &JsValue::from_str(&mode.to_string()),
                )?;
            }
            Response::Level(sub, value) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"level".into())?;
                js_sys::Reflect::set(&obj, &"sub".into(), &JsValue::from_f64(sub as f64))?;
                js_sys::Reflect::set(&obj, &"value".into(), &JsValue::from_f64(value as f64))?;
            }
            Response::Meter(sub, value) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"meter".into())?;
                js_sys::Reflect::set(&obj, &"sub".into(), &JsValue::from_f64(sub as f64))?;
                js_sys::Reflect::set(&obj, &"value".into(), &JsValue::from_f64(value as f64))?;
            }
            Response::TransceiverId(id) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"transceiver_id".into())?;
                js_sys::Reflect::set(&obj, &"id".into(), &JsValue::from_f64(id as f64))?;
            }
            Response::Various(sub, value) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"various".into())?;
                js_sys::Reflect::set(&obj, &"sub".into(), &JsValue::from_f64(sub as f64))?;
                js_sys::Reflect::set(&obj, &"value".into(), &JsValue::from_f64(value as f64))?;
            }
            Response::Duplex(dir) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"duplex".into())?;
                let label = match dir {
                    0x10 => "simplex",
                    0x11 => "dup-",
                    0x12 => "dup+",
                    _ => "unknown",
                };
                js_sys::Reflect::set(&obj, &"direction".into(), &JsValue::from_str(label))?;
                js_sys::Reflect::set(&obj, &"raw".into(), &JsValue::from_f64(dir as f64))?;
            }
            Response::Offset(freq) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"offset".into())?;
                js_sys::Reflect::set(&obj, &"hz".into(), &JsValue::from_f64(freq.hz() as f64))?;
                js_sys::Reflect::set(
                    &obj,
                    &"display".into(),
                    &JsValue::from_str(&freq.to_string()),
                )?;
            }
            Response::ToneFrequency(sub, freq_tenths) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"tone_frequency".into())?;
                js_sys::Reflect::set(&obj, &"sub".into(), &JsValue::from_f64(sub as f64))?;
                js_sys::Reflect::set(
                    &obj,
                    &"tenths_hz".into(),
                    &JsValue::from_f64(freq_tenths as f64),
                )?;
            }
            Response::DtcsCode(tx_pol, rx_pol, code) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"dtcs".into())?;
                js_sys::Reflect::set(
                    &obj,
                    &"tx_polarity".into(),
                    &JsValue::from_f64(tx_pol as f64),
                )?;
                js_sys::Reflect::set(
                    &obj,
                    &"rx_polarity".into(),
                    &JsValue::from_f64(rx_pol as f64),
                )?;
                js_sys::Reflect::set(&obj, &"code".into(), &JsValue::from_f64(code as f64))?;
            }
            Response::GpsPosition(gps) => {
                js_sys::Reflect::set(&obj, &"type".into(), &"gps".into())?;
                // Convert to decimal degrees for JS
                let lat = gps.lat_deg as f64
                    + (gps.lat_min as f64 + gps.lat_min_frac as f64 / 1000.0) / 60.0;
                let lat = if gps.lat_north { lat } else { -lat };
                let lon = gps.lon_deg as f64
                    + (gps.lon_min as f64 + gps.lon_min_frac as f64 / 1000.0) / 60.0;
                let lon = if gps.lon_east { lon } else { -lon };
                let alt = gps.alt_tenths as f64 / 10.0;
                let alt = if gps.alt_negative { -alt } else { alt };

                js_sys::Reflect::set(&obj, &"latitude".into(), &JsValue::from_f64(lat))?;
                js_sys::Reflect::set(&obj, &"longitude".into(), &JsValue::from_f64(lon))?;
                js_sys::Reflect::set(&obj, &"altitude_m".into(), &JsValue::from_f64(alt))?;
                js_sys::Reflect::set(
                    &obj,
                    &"course".into(),
                    &JsValue::from_f64(gps.course as f64),
                )?;
                js_sys::Reflect::set(
                    &obj,
                    &"speed_kmh".into(),
                    &JsValue::from_f64(gps.speed_tenths as f64 / 10.0),
                )?;
            }
        }

        Ok(obj.into())
    }

    /// Try to infer a Response from a frame without knowing the command context.
    fn infer_response(&self, frame: &Frame) -> Result<Response, JsValue> {
        if frame.is_ok() {
            return Ok(Response::Ok);
        }
        if frame.is_ng() {
            return Ok(Response::Ng);
        }

        // Try to infer based on command byte
        let dummy_cmd = match frame.command {
            cmd::READ_FREQ | cmd::SET_FREQ => Command::ReadFrequency,
            cmd::READ_MODE => Command::ReadMode,
            cmd::LEVEL => {
                let sub = frame.sub_command.unwrap_or(0);
                Command::ReadLevel(sub)
            }
            cmd::METER => {
                let sub = frame.sub_command.unwrap_or(0);
                Command::ReadMeter(sub)
            }
            cmd::READ_ID => Command::ReadTransceiverId,
            cmd::VARIOUS => {
                let sub = frame.sub_command.unwrap_or(0);
                Command::ReadVarious(sub)
            }
            cmd::READ_DUPLEX => Command::ReadDuplex,
            cmd::READ_OFFSET => Command::ReadOffset,
            cmd::TONE => {
                let sub = frame.sub_command.unwrap_or(0);
                Command::ReadTone(sub)
            }
            cmd::READ_GPS => Command::ReadGpsPosition,
            _ => return Err(JsValue::from_str(&format!("unknown command byte: {:#04x}", frame.command))),
        };

        response::parse_response(frame, &dummy_cmd)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Encode a command into raw CI-V bytes ready to send over WebSerial.
#[wasm_bindgen]
pub fn encode_command(cmd_name: &str, arg_json: &str) -> Result<Vec<u8>, JsValue> {
    let command = parse_command(cmd_name, arg_json)?;
    let frame = command
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set frequency" command. Frequency in Hz.
#[wasm_bindgen]
pub fn encode_set_frequency(hz: f64) -> Result<Vec<u8>, JsValue> {
    let freq =
        Frequency::from_hz(hz as u64).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let frame = Command::SetFrequency(freq)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read frequency" command.
#[wasm_bindgen]
pub fn encode_read_frequency() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadFrequency
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read mode" command.
#[wasm_bindgen]
pub fn encode_read_mode() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadMode
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set mode" command.
#[wasm_bindgen]
pub fn encode_set_mode(mode: &str) -> Result<Vec<u8>, JsValue> {
    let operating_mode = match mode.to_uppercase().as_str() {
        "FM" => OperatingMode::Fm,
        "FM-N" | "FMN" => OperatingMode::FmN,
        "AM" => OperatingMode::Am,
        "AM-N" | "AMN" => OperatingMode::AmN,
        "DV" => OperatingMode::Dv,
        _ => return Err(JsValue::from_str(&format!("unknown mode: {mode}"))),
    };
    let frame = Command::SetMode(operating_mode)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "select VFO" command. Pass "A" or "B".
#[wasm_bindgen]
pub fn encode_select_vfo(vfo: &str) -> Result<Vec<u8>, JsValue> {
    let command = match vfo.to_uppercase().as_str() {
        "A" => Command::SelectVfoA,
        "B" => Command::SelectVfoB,
        _ => return Err(JsValue::from_str(&format!("unknown VFO: {vfo}, use A or B"))),
    };
    let frame = command
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "power on" command.
#[wasm_bindgen]
pub fn encode_power_on() -> Result<Vec<u8>, JsValue> {
    let frame = Command::PowerOn
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "power off" command.
#[wasm_bindgen]
pub fn encode_power_off() -> Result<Vec<u8>, JsValue> {
    let frame = Command::PowerOff
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read level" command. Sub-command: 0x01=AF, 0x02=RF gain, 0x03=squelch, 0x0A=RF power.
#[wasm_bindgen]
pub fn encode_read_level(sub: u8) -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadLevel(sub)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set level" command.
#[wasm_bindgen]
pub fn encode_set_level(sub: u8, value: u16) -> Result<Vec<u8>, JsValue> {
    let frame = Command::SetLevel(sub, value)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read S-meter" command.
#[wasm_bindgen]
pub fn encode_read_s_meter() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadMeter(meter_sub::S_METER)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read GPS position" command.
#[wasm_bindgen]
pub fn encode_read_gps() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadGpsPosition
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read tone mode" command (reads the tone/squelch function: off, tone, TSQL, DTCS, etc.).
#[wasm_bindgen]
pub fn encode_read_tone_mode() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadVarious(various_sub::TONE_SQUELCH_FUNC)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set tone mode" command. mode: 0=Off, 1=Tone, 2=TSQL, 3=DTCS.
#[wasm_bindgen]
pub fn encode_set_tone_mode(mode: u8) -> Result<Vec<u8>, JsValue> {
    let frame = Command::SetVarious(various_sub::TONE_SQUELCH_FUNC, mode)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read repeater tone" (Tx tone) command.
#[wasm_bindgen]
pub fn encode_read_tx_tone() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadTone(tone_sub::REPEATER_TONE)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read TSQL tone" (Rx tone) command.
#[wasm_bindgen]
pub fn encode_read_rx_tone() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadTone(tone_sub::TSQL_TONE)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set repeater tone" (Tx tone) command. freq_tenths = frequency in 0.1 Hz (e.g. 1413 = 141.3 Hz).
#[wasm_bindgen]
pub fn encode_set_tx_tone(freq_tenths: u16) -> Result<Vec<u8>, JsValue> {
    let frame = Command::SetTone(tone_sub::REPEATER_TONE, freq_tenths)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set TSQL tone" (Rx tone) command. freq_tenths = frequency in 0.1 Hz.
#[wasm_bindgen]
pub fn encode_set_rx_tone(freq_tenths: u16) -> Result<Vec<u8>, JsValue> {
    let frame = Command::SetTone(tone_sub::TSQL_TONE, freq_tenths)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "read DTCS code" command.
#[wasm_bindgen]
pub fn encode_read_dtcs() -> Result<Vec<u8>, JsValue> {
    let frame = Command::ReadTone(tone_sub::DTCS)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Encode a "set DTCS code" command. tx_pol/rx_pol: 0=Normal, 1=Reverse.
#[wasm_bindgen]
pub fn encode_set_dtcs(tx_pol: u8, rx_pol: u8, code: u16) -> Result<Vec<u8>, JsValue> {
    let frame = Command::SetDtcs(tx_pol, rx_pol, code)
        .to_frame()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(frame.to_bytes())
}

/// Generic command builder from name and JSON arg string.
fn parse_command(cmd_name: &str, arg_json: &str) -> Result<Command, JsValue> {
    match cmd_name {
        "read_frequency" => Ok(Command::ReadFrequency),
        "set_frequency" => {
            let hz: f64 = arg_json
                .parse()
                .map_err(|_| JsValue::from_str("invalid frequency Hz value"))?;
            let freq =
                Frequency::from_hz(hz as u64).map_err(|e| JsValue::from_str(&e.to_string()))?;
            Ok(Command::SetFrequency(freq))
        }
        "read_mode" => Ok(Command::ReadMode),
        "set_mode" => {
            let mode = match arg_json.to_uppercase().as_str() {
                "FM" => OperatingMode::Fm,
                "FM-N" | "FMN" => OperatingMode::FmN,
                "AM" => OperatingMode::Am,
                "AM-N" | "AMN" => OperatingMode::AmN,
                "DV" => OperatingMode::Dv,
                _ => return Err(JsValue::from_str(&format!("unknown mode: {arg_json}"))),
            };
            Ok(Command::SetMode(mode))
        }
        "select_vfo_a" => Ok(Command::SelectVfoA),
        "select_vfo_b" => Ok(Command::SelectVfoB),
        "power_on" => Ok(Command::PowerOn),
        "power_off" => Ok(Command::PowerOff),
        "read_level" => {
            let sub: u8 = arg_json
                .parse()
                .map_err(|_| JsValue::from_str("invalid level sub-command"))?;
            Ok(Command::ReadLevel(sub))
        }
        "read_s_meter" => Ok(Command::ReadMeter(meter_sub::S_METER)),
        "read_gps" => Ok(Command::ReadGpsPosition),
        _ => Err(JsValue::from_str(&format!("unknown command: {cmd_name}"))),
    }
}

