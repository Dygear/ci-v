use crate::bcd;
use crate::command::{Command, cmd};
use crate::error::{CivError, Result};
use crate::frequency::Frequency;
use crate::mode::OperatingMode;
use crate::protocol::Frame;

/// A typed response from the radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Command accepted (FB response).
    Ok,
    /// Command rejected (FA response).
    Ng,
    /// Frequency data (response to ReadFrequency or echo of SetFrequency).
    Frequency(Frequency),
    /// Operating mode and filter (response to ReadMode).
    Mode(OperatingMode),
    /// Level value (response to ReadLevel). Contains (sub_command, value).
    Level(u8, u16),
    /// Meter reading (response to ReadMeter). Contains (sub_command, value).
    Meter(u8, u16),
    /// Transceiver ID (response to ReadTransceiverId).
    TransceiverId(u8),
    /// Various function setting (response to ReadVarious). Contains (sub_command, raw_value).
    /// The value is a single raw byte, NOT BCD-decoded.
    Various(u8, u8),
    /// Duplex direction (response to ReadDuplex).
    /// 0x10=Simplex, 0x11=DUP-, 0x12=DUP+. The sub_command IS the data.
    Duplex(u8),
    /// Offset frequency (response to ReadOffset). Same 5-byte LE BCD as operating frequency.
    Offset(Frequency),
    /// Tone frequency (response to ReadTone 0x00 or 0x01).
    /// Contains (sub_command, frequency in tenths of Hz, e.g. 1413 = 141.3 Hz).
    ToneFrequency(u8, u16),
    /// DTCS code and polarity (response to ReadTone 0x02).
    /// Contains (tx_polarity, rx_polarity, code). Polarity: 0=Normal, 1=Reverse.
    DtcsCode(u8, u8, u16),
}

/// Parse a response `Frame` into a typed `Response`, using the original `Command`
/// to disambiguate commands that share the same command byte.
pub fn parse_response(frame: &Frame, command: &Command) -> Result<Response> {
    // Handle OK/NG first — these apply to any command.
    if frame.is_ok() {
        return Ok(Response::Ok);
    }
    if frame.is_ng() {
        return Ok(Response::Ng);
    }

    match command {
        Command::ReadFrequency => parse_frequency_response(frame),
        Command::SetFrequency(_) => {
            // SetFrequency gets an OK/NG (handled above), but we could
            // also receive a frequency echo.
            if frame.command == cmd::SET_FREQ || frame.command == cmd::READ_FREQ {
                parse_frequency_response(frame)
            } else {
                Err(CivError::InvalidFrame)
            }
        }
        Command::ReadMode => parse_mode_response(frame),
        Command::SetMode(_) => Ok(Response::Ok),
        Command::SelectVfoA | Command::SelectVfoB => Ok(Response::Ok),
        Command::ReadLevel(sub) => parse_level_response(frame, *sub),
        Command::SetLevel(_, _) => Ok(Response::Ok),
        Command::ReadMeter(sub) => parse_meter_response(frame, *sub),
        Command::PowerOn | Command::PowerOff => Ok(Response::Ok),
        Command::ReadTransceiverId => parse_transceiver_id_response(frame),
        Command::ReadVarious(sub) => parse_various_response(frame, *sub),
        Command::ReadDuplex => parse_duplex_response(frame),
        Command::ReadOffset => parse_offset_response(frame),
        Command::ReadTone(sub) => parse_tone_response(frame, *sub),
        Command::SetVarious(_, _) => Ok(Response::Ok),
        Command::SetTone(_, _) => Ok(Response::Ok),
        Command::SetDtcs(_, _, _) => Ok(Response::Ok),
    }
}

/// Parse a frequency response frame.
///
/// The frequency is encoded as 5 BCD bytes in the frame payload.
/// In a frequency response, the payload is everything after the command byte:
/// `sub_command` (if present) + `data`.
fn parse_frequency_response(frame: &Frame) -> Result<Response> {
    let mut freq_bytes = Vec::with_capacity(5);
    if let Some(sc) = frame.sub_command {
        freq_bytes.push(sc);
    }
    freq_bytes.extend_from_slice(&frame.data);

    if freq_bytes.len() != 5 {
        return Err(CivError::InvalidFrame);
    }

    let mut arr = [0u8; 5];
    arr.copy_from_slice(&freq_bytes);
    let freq = Frequency::from_civ_bytes(arr)?;
    Ok(Response::Frequency(freq))
}

/// Parse a mode response frame.
///
/// Mode response payload: `<mode_byte> <filter_byte>`
fn parse_mode_response(frame: &Frame) -> Result<Response> {
    // The mode response has the mode byte as sub_command and filter as data[0].
    let mode_byte = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    let filter_byte = frame.data.first().copied().ok_or(CivError::InvalidFrame)?;
    let mode = OperatingMode::from_civ_bytes(mode_byte, filter_byte)?;
    Ok(Response::Mode(mode))
}

/// Parse a level response frame.
fn parse_level_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    if frame.data.len() != 2 {
        return Err(CivError::InvalidFrame);
    }
    let value = bcd::decode_bcd_be(&frame.data)? as u16;
    Ok(Response::Level(sub, value))
}

/// Parse a meter response frame.
fn parse_meter_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    if frame.data.len() != 2 {
        return Err(CivError::InvalidFrame);
    }
    let value = bcd::decode_bcd_be(&frame.data)? as u16;
    Ok(Response::Meter(sub, value))
}

/// Parse a transceiver ID response frame.
fn parse_transceiver_id_response(frame: &Frame) -> Result<Response> {
    let id = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    Ok(Response::TransceiverId(id))
}

/// Parse a various function response frame.
///
/// The response is a single raw byte (NOT BCD-decoded).
/// Frame format: `[cmd=0x16] [sub=0x5D] [data: 1 byte raw value]`
fn parse_various_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    let value = frame.data.first().copied().ok_or(CivError::InvalidFrame)?;
    Ok(Response::Various(sub, value))
}

/// Parse a duplex direction response frame.
///
/// The sub_command byte IS the data: 0x10=Simplex, 0x11=DUP-, 0x12=DUP+.
fn parse_duplex_response(frame: &Frame) -> Result<Response> {
    let duplex = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    Ok(Response::Duplex(duplex))
}

/// Parse a duplex offset frequency response frame.
///
/// Same 5-byte LE BCD format as the operating frequency.
fn parse_offset_response(frame: &Frame) -> Result<Response> {
    // Reuse the same logic as frequency parsing.
    let mut freq_bytes = Vec::with_capacity(5);
    if let Some(sc) = frame.sub_command {
        freq_bytes.push(sc);
    }
    freq_bytes.extend_from_slice(&frame.data);

    if freq_bytes.len() != 5 {
        return Err(CivError::InvalidFrame);
    }

    let mut arr = [0u8; 5];
    arr.copy_from_slice(&freq_bytes);
    let freq = Frequency::from_civ_bytes(arr)?;
    Ok(Response::Offset(freq))
}

/// Parse a tone/DTCS response frame.
///
/// For sub 0x00 (Tx tone) and 0x01 (Rx tone):
///   3 bytes: `[0x00, hundreds_tens_BCD, units_tenths_BCD]`
///   Example: 141.3 Hz → `[0x00, 0x14, 0x13]` → stored as 1413 (tenths of Hz).
///
/// For sub 0x02 (DTCS):
///   3 bytes: `[polarity_nibbles, 0x0_first_digit, second_third_BCD]`
///   High nibble of byte 0 = Tx polarity (0=Normal, 1=Reverse)
///   Low nibble of byte 0 = Rx polarity
///   Example: code 023, normal → `[0x00, 0x00, 0x23]`
fn parse_tone_response(frame: &Frame, expected_sub: u8) -> Result<Response> {
    let sub = frame.sub_command.ok_or(CivError::InvalidFrame)?;
    if sub != expected_sub {
        return Err(CivError::InvalidFrame);
    }
    if frame.data.len() != 3 {
        return Err(CivError::InvalidFrame);
    }

    match sub {
        0x00 | 0x01 => {
            // Tone frequency: [0x00, hundreds_tens, units_tenths]
            let hundreds_tens = frame.data[1];
            let units_tenths = frame.data[2];
            let ht = bcd::decode_bcd_be(&[hundreds_tens])? as u16;
            let ut = bcd::decode_bcd_be(&[units_tenths])? as u16;
            let freq_tenths = ht * 100 + ut;
            Ok(Response::ToneFrequency(sub, freq_tenths))
        }
        0x02 => {
            // DTCS code: [polarity, first_digit, second_third]
            let polarity_byte = frame.data[0];
            let tx_pol = (polarity_byte >> 4) & 0x0F;
            let rx_pol = polarity_byte & 0x0F;
            let first = bcd::decode_bcd_be(&[frame.data[1]])? as u16;
            let second_third = bcd::decode_bcd_be(&[frame.data[2]])? as u16;
            let code = first * 100 + second_third;
            Ok(Response::DtcsCode(tx_pol, rx_pol, code))
        }
        _ => Err(CivError::InvalidFrame),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::level_sub;
    use crate::command::meter_sub;
    use crate::protocol::{ADDR_CONTROLLER, ADDR_ID52, NG, OK};

    fn make_response_frame(command: u8, sub_command: Option<u8>, data: Vec<u8>) -> Frame {
        Frame {
            dst: ADDR_CONTROLLER,
            src: ADDR_ID52,
            command,
            sub_command,
            data,
        }
    }

    #[test]
    fn test_parse_ok() {
        let frame = make_response_frame(OK, None, vec![]);
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(resp, Response::Ok);
    }

    #[test]
    fn test_parse_ng() {
        let frame = make_response_frame(NG, None, vec![]);
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(resp, Response::Ng);
    }

    #[test]
    fn test_parse_frequency() {
        // 145.000.000 Hz: BCD LE = 00 00 00 45 01
        let frame = make_response_frame(cmd::READ_FREQ, Some(0x00), vec![0x00, 0x00, 0x45, 0x01]);
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(
            resp,
            Response::Frequency(Frequency::from_hz(145_000_000).unwrap())
        );
    }

    #[test]
    fn test_parse_mode_fm() {
        let frame = make_response_frame(cmd::READ_MODE, Some(0x05), vec![0x01]);
        let resp = parse_response(&frame, &Command::ReadMode).unwrap();
        assert_eq!(resp, Response::Mode(OperatingMode::Fm));
    }

    #[test]
    fn test_parse_mode_dv() {
        let frame = make_response_frame(cmd::READ_MODE, Some(0x17), vec![0x01]);
        let resp = parse_response(&frame, &Command::ReadMode).unwrap();
        assert_eq!(resp, Response::Mode(OperatingMode::Dv));
    }

    #[test]
    fn test_parse_level() {
        let frame = make_response_frame(cmd::LEVEL, Some(level_sub::AF_LEVEL), vec![0x01, 0x28]);
        let resp = parse_response(&frame, &Command::ReadLevel(level_sub::AF_LEVEL)).unwrap();
        assert_eq!(resp, Response::Level(level_sub::AF_LEVEL, 128));
    }

    #[test]
    fn test_parse_meter() {
        let frame = make_response_frame(cmd::METER, Some(meter_sub::S_METER), vec![0x00, 0x50]);
        let resp = parse_response(&frame, &Command::ReadMeter(meter_sub::S_METER)).unwrap();
        assert_eq!(resp, Response::Meter(meter_sub::S_METER, 50));
    }

    #[test]
    fn test_parse_transceiver_id() {
        let frame = make_response_frame(cmd::READ_ID, Some(0xB4), vec![]);
        let resp = parse_response(&frame, &Command::ReadTransceiverId).unwrap();
        assert_eq!(resp, Response::TransceiverId(0xB4));
    }

    #[test]
    fn test_parse_level_wrong_sub() {
        let frame = make_response_frame(cmd::LEVEL, Some(0x99), vec![0x01, 0x28]);
        let result = parse_response(&frame, &Command::ReadLevel(level_sub::AF_LEVEL));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_various_tone_squelch() {
        use crate::command::various_sub;
        // Tone squelch function = 0x01 (TONE Tx)
        let frame = make_response_frame(
            cmd::VARIOUS,
            Some(various_sub::TONE_SQUELCH_FUNC),
            vec![0x01],
        );
        let resp = parse_response(
            &frame,
            &Command::ReadVarious(various_sub::TONE_SQUELCH_FUNC),
        )
        .unwrap();
        assert_eq!(
            resp,
            Response::Various(various_sub::TONE_SQUELCH_FUNC, 0x01)
        );
    }

    #[test]
    fn test_parse_duplex_simplex() {
        let frame = make_response_frame(cmd::READ_DUPLEX, Some(0x10), vec![]);
        let resp = parse_response(&frame, &Command::ReadDuplex).unwrap();
        assert_eq!(resp, Response::Duplex(0x10));
    }

    #[test]
    fn test_parse_duplex_plus() {
        let frame = make_response_frame(cmd::READ_DUPLEX, Some(0x12), vec![]);
        let resp = parse_response(&frame, &Command::ReadDuplex).unwrap();
        assert_eq!(resp, Response::Duplex(0x12));
    }

    #[test]
    fn test_parse_offset() {
        // 600 kHz = 600,000 Hz → BCD LE: [0x00, 0x00, 0x60, 0x00, 0x00]
        let frame = make_response_frame(cmd::READ_OFFSET, Some(0x00), vec![0x00, 0x60, 0x00, 0x00]);
        let resp = parse_response(&frame, &Command::ReadOffset).unwrap();
        assert_eq!(resp, Response::Offset(Frequency::from_hz(600_000).unwrap()));
    }

    #[test]
    fn test_parse_offset_5mhz() {
        // 5 MHz = 5,000,000 Hz → BCD LE: [0x00, 0x00, 0x00, 0x05, 0x00]
        let frame = make_response_frame(cmd::READ_OFFSET, Some(0x00), vec![0x00, 0x00, 0x05, 0x00]);
        let resp = parse_response(&frame, &Command::ReadOffset).unwrap();
        assert_eq!(
            resp,
            Response::Offset(Frequency::from_hz(5_000_000).unwrap())
        );
    }

    #[test]
    fn test_parse_tone_frequency_tx() {
        use crate::command::tone_sub;
        // 141.3 Hz → [0x00, 0x14, 0x13]
        let frame = make_response_frame(
            cmd::TONE,
            Some(tone_sub::REPEATER_TONE),
            vec![0x00, 0x14, 0x13],
        );
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::REPEATER_TONE)).unwrap();
        assert_eq!(resp, Response::ToneFrequency(tone_sub::REPEATER_TONE, 1413));
    }

    #[test]
    fn test_parse_tone_frequency_rx() {
        use crate::command::tone_sub;
        // 88.5 Hz → [0x00, 0x08, 0x85]
        let frame =
            make_response_frame(cmd::TONE, Some(tone_sub::TSQL_TONE), vec![0x00, 0x08, 0x85]);
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::TSQL_TONE)).unwrap();
        assert_eq!(resp, Response::ToneFrequency(tone_sub::TSQL_TONE, 885));
    }

    #[test]
    fn test_parse_dtcs_code() {
        use crate::command::tone_sub;
        // Code 023, normal polarity → [0x00, 0x00, 0x23]
        let frame = make_response_frame(cmd::TONE, Some(tone_sub::DTCS), vec![0x00, 0x00, 0x23]);
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::DTCS)).unwrap();
        assert_eq!(resp, Response::DtcsCode(0, 0, 23));
    }

    #[test]
    fn test_parse_dtcs_code_reverse() {
        use crate::command::tone_sub;
        // Code 754, Tx=Reverse, Rx=Normal → [0x10, 0x07, 0x54]
        let frame = make_response_frame(cmd::TONE, Some(tone_sub::DTCS), vec![0x10, 0x07, 0x54]);
        let resp = parse_response(&frame, &Command::ReadTone(tone_sub::DTCS)).unwrap();
        assert_eq!(resp, Response::DtcsCode(1, 0, 754));
    }
}
