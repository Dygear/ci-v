use crate::error::{CivError, Result};

/// CI-V frame preamble byte.
pub const PREAMBLE: u8 = 0xFE;
/// CI-V frame end-of-message byte.
pub const EOM: u8 = 0xFD;
/// CI-V OK response command byte.
pub const OK: u8 = 0xFB;
/// CI-V NG (error) response command byte.
pub const NG: u8 = 0xFA;

/// Default CI-V address for the ID-52A Plus.
pub const ADDR_ID52: u8 = 0xB4;
/// Default CI-V address for the controller (PC).
pub const ADDR_CONTROLLER: u8 = 0xE0;

/// A parsed CI-V frame.
///
/// Frame wire format: `FE FE <dst> <src> <cmd> [<sub_cmd>] [<data>...] FD`
///
/// The `sub_command` and `data` fields are optional and depend on the command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub dst: u8,
    pub src: u8,
    pub command: u8,
    pub sub_command: Option<u8>,
    pub data: Vec<u8>,
}

impl Frame {
    /// Create a new frame from the controller to the radio.
    pub fn new(command: u8, sub_command: Option<u8>, data: Vec<u8>) -> Self {
        Self {
            dst: ADDR_ID52,
            src: ADDR_CONTROLLER,
            command,
            sub_command,
            data,
        }
    }

    /// Serialize the frame to its wire representation.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + self.data.len());
        bytes.push(PREAMBLE);
        bytes.push(PREAMBLE);
        bytes.push(self.dst);
        bytes.push(self.src);
        bytes.push(self.command);
        if let Some(sc) = self.sub_command {
            bytes.push(sc);
        }
        bytes.extend_from_slice(&self.data);
        bytes.push(EOM);
        bytes
    }

    /// Parse a CI-V frame from a byte buffer.
    ///
    /// Returns the parsed frame and the number of bytes consumed.
    /// Returns `None` if the buffer does not contain a complete frame.
    /// Returns `Err` if the buffer contains an invalid frame.
    pub fn parse(buf: &[u8]) -> Result<Option<(Frame, usize)>> {
        // Find the start of a frame (two consecutive FE bytes).
        let start = match buf.windows(2).position(|w| w[0] == PREAMBLE && w[1] == PREAMBLE) {
            Some(pos) => pos,
            None => return Ok(None),
        };

        // Find the end-of-message byte after the preamble.
        let eom_pos = match buf[start..].iter().position(|&b| b == EOM) {
            Some(pos) => start + pos,
            None => return Ok(None),
        };

        // Minimum frame: FE FE dst src cmd FD = 6 bytes
        let frame_bytes = &buf[start..=eom_pos];
        if frame_bytes.len() < 6 {
            return Err(CivError::InvalidFrame);
        }

        let dst = frame_bytes[2];
        let src = frame_bytes[3];
        let command = frame_bytes[4];

        // The payload is everything between the command byte and the EOM byte.
        let payload = &frame_bytes[5..frame_bytes.len() - 1];

        // For OK/NG responses, there's no sub_command or data.
        let (sub_command, data) = if command == OK || command == NG || payload.is_empty() {
            (None, Vec::new())
        } else if payload.len() == 1 {
            // Single byte payload: could be a sub_command with no data,
            // or data with no sub_command. We treat it as sub_command.
            (Some(payload[0]), Vec::new())
        } else {
            // First byte is sub_command, rest is data.
            (Some(payload[0]), payload[1..].to_vec())
        };

        let consumed = eom_pos + 1 - start;
        Ok(Some((
            Frame {
                dst,
                src,
                command,
                sub_command,
                data,
            },
            consumed,
        )))
    }

    /// Returns `true` if this is an OK response frame.
    pub fn is_ok(&self) -> bool {
        self.command == OK
    }

    /// Returns `true` if this is an NG (error) response frame.
    pub fn is_ng(&self) -> bool {
        self.command == NG
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_response_parse() {
        let bytes = [0xFE, 0xFE, ADDR_CONTROLLER, ADDR_ID52, OK, EOM];
        let (frame, consumed) = Frame::parse(&bytes).unwrap().unwrap();
        assert_eq!(consumed, 6);
        assert!(frame.is_ok());
        assert_eq!(frame.dst, ADDR_CONTROLLER);
        assert_eq!(frame.src, ADDR_ID52);
        assert_eq!(frame.command, OK);
        assert_eq!(frame.sub_command, None);
        assert!(frame.data.is_empty());
    }

    #[test]
    fn test_ng_response_parse() {
        let bytes = [0xFE, 0xFE, ADDR_CONTROLLER, ADDR_ID52, NG, EOM];
        let (frame, consumed) = Frame::parse(&bytes).unwrap().unwrap();
        assert_eq!(consumed, 6);
        assert!(frame.is_ng());
    }

    #[test]
    fn test_roundtrip_simple() {
        let frame = Frame::new(0x03, None, vec![]);
        let bytes = frame.to_bytes();
        let (parsed, consumed) = Frame::parse(&bytes).unwrap().unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(parsed.dst, frame.dst);
        assert_eq!(parsed.src, frame.src);
        assert_eq!(parsed.command, frame.command);
    }

    #[test]
    fn test_roundtrip_with_sub_and_data() {
        let frame = Frame::new(0x14, Some(0x01), vec![0x01, 0x28]);
        let bytes = frame.to_bytes();
        let (parsed, _) = Frame::parse(&bytes).unwrap().unwrap();
        assert_eq!(parsed.command, 0x14);
        assert_eq!(parsed.sub_command, Some(0x01));
        assert_eq!(parsed.data, vec![0x01, 0x28]);
    }

    #[test]
    fn test_parse_frequency_response() {
        // Simulated frequency response for 145.000.000 Hz
        let bytes = [
            0xFE, 0xFE, ADDR_CONTROLLER, ADDR_ID52,
            0x03, // command: read frequency response
            0x00, 0x00, 0x00, 0x45, 0x01, // BCD freq data (treated as sub + data)
            EOM,
        ];
        let (frame, _) = Frame::parse(&bytes).unwrap().unwrap();
        assert_eq!(frame.command, 0x03);
        // First payload byte becomes sub_command, rest is data
        assert_eq!(frame.sub_command, Some(0x00));
        assert_eq!(frame.data, vec![0x00, 0x00, 0x45, 0x01]);
    }

    #[test]
    fn test_parse_no_complete_frame() {
        let bytes = [0xFE, 0xFE, 0xB4, 0xE0, 0x03];
        assert!(Frame::parse(&bytes).unwrap().is_none());
    }

    #[test]
    fn test_parse_empty_buffer() {
        let bytes = [];
        assert!(Frame::parse(&bytes).unwrap().is_none());
    }

    #[test]
    fn test_parse_garbage_before_frame() {
        let bytes = [
            0x00, 0xFF, // garbage
            0xFE, 0xFE, ADDR_CONTROLLER, ADDR_ID52, OK, EOM,
        ];
        let (frame, consumed) = Frame::parse(&bytes).unwrap().unwrap();
        // consumed counts from start of FE FE to end of FD
        assert_eq!(consumed, 6);
        assert!(frame.is_ok());
    }

    #[test]
    fn test_to_bytes_format() {
        let frame = Frame::new(0x03, None, vec![]);
        let bytes = frame.to_bytes();
        assert_eq!(bytes, vec![0xFE, 0xFE, ADDR_ID52, ADDR_CONTROLLER, 0x03, EOM]);
    }

    #[test]
    fn test_to_bytes_with_data() {
        let frame = Frame::new(0x05, None, vec![0x00, 0x00, 0x00, 0x50, 0x14]);
        let bytes = frame.to_bytes();
        assert_eq!(
            bytes,
            vec![0xFE, 0xFE, ADDR_ID52, ADDR_CONTROLLER, 0x05, 0x00, 0x00, 0x00, 0x50, 0x14, EOM]
        );
    }
}
