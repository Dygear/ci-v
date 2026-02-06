use crate::bcd;
use crate::command::{cmd, Command};
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
        Command::SelectVfoA | Command::SelectVfoB | Command::ExchangeVfo => Ok(Response::Ok),
        Command::ReadLevel(sub) => parse_level_response(frame, *sub),
        Command::SetLevel(_, _) => Ok(Response::Ok),
        Command::ReadMeter(sub) => parse_meter_response(frame, *sub),
        Command::PowerOn | Command::PowerOff => Ok(Response::Ok),
        Command::ReadTransceiverId => parse_transceiver_id_response(frame),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::level_sub;
    use crate::command::meter_sub;
    use crate::protocol::{ADDR_CONTROLLER, ADDR_ID52, OK, NG};

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
        let frame = make_response_frame(
            cmd::READ_FREQ,
            Some(0x00),
            vec![0x00, 0x00, 0x45, 0x01],
        );
        let resp = parse_response(&frame, &Command::ReadFrequency).unwrap();
        assert_eq!(resp, Response::Frequency(Frequency::from_hz(145_000_000).unwrap()));
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
}
