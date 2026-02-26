use std::io;
use std::time::Duration;

use log::{debug, info, warn};
use serialport::SerialPortType;

use crate::command::Command;
use crate::error::{CivError, Result};
use crate::protocol::{Frame, PREAMBLE};

use super::Transport;

/// USB product string to match for the ID-52A Plus.
const ID52_PRODUCT: &str = "ID-52PLUS";

/// Default serial port settings.
const DATA_BITS: serialport::DataBits = serialport::DataBits::Eight;
const STOP_BITS: serialport::StopBits = serialport::StopBits::One;
const PARITY: serialport::Parity = serialport::Parity::None;

/// Baud rates to try during auto-detection (most common first).
const BAUD_RATES: &[u32] = &[19200, 9600, 4800];

/// A CI-V transport backed by a native serial port.
pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialTransport {
    pub fn new(port: Box<dyn serialport::SerialPort>) -> Self {
        Self { port }
    }
}

impl Transport for SerialTransport {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        io::Write::write_all(&mut self.port, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.port)
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.port, buf)
    }

    fn set_read_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.port
            .set_timeout(timeout)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

/// Find the serial port for an ID-52A Plus radio.
///
/// Scans all available serial ports and looks for a USB port whose
/// product string contains "ID-52PLUS".
pub fn find_id52_port() -> Result<String> {
    let ports = serialport::available_ports().map_err(CivError::Serial)?;

    for port in &ports {
        debug!("found port: {} ({:?})", port.port_name, port.port_type);
        if let SerialPortType::UsbPort(usb_info) = &port.port_type
            && let Some(product) = &usb_info.product
            && product.contains(ID52_PRODUCT)
        {
            info!("found ID-52A Plus on {}", port.port_name);
            return Ok(port.port_name.clone());
        }
    }

    // Log available ports for troubleshooting.
    if ports.is_empty() {
        warn!("no serial ports found");
    } else {
        warn!("ID-52A Plus not found among {} port(s):", ports.len());
        for port in &ports {
            warn!("  {} ({:?})", port.port_name, port.port_type);
        }
    }

    Err(CivError::PortNotFound)
}

/// Open a serial port with CI-V settings (8N1) at the given baud rate.
pub fn open_port(port_name: &str, baud_rate: u32) -> Result<SerialTransport> {
    let port = serialport::new(port_name, baud_rate)
        .data_bits(DATA_BITS)
        .stop_bits(STOP_BITS)
        .parity(PARITY)
        .timeout(Duration::from_millis(500))
        .open()
        .map_err(CivError::Serial)?;

    info!("opened {} at {} baud", port_name, baud_rate);
    Ok(SerialTransport::new(port))
}

/// Try to auto-detect the baud rate by sending a ReadTransceiverId command
/// at each candidate rate and checking for a valid response.
///
/// Returns the working baud rate and open transport on success.
pub fn auto_detect_baud(port_name: &str) -> Result<(u32, SerialTransport)> {
    let cmd_frame = Command::ReadTransceiverId.to_frame()?;
    let cmd_bytes = cmd_frame.to_bytes();

    for &baud in BAUD_RATES {
        debug!("trying {} baud on {}", baud, port_name);

        let mut transport = match open_port(port_name, baud) {
            Ok(t) => t,
            Err(e) => {
                warn!("failed to open at {} baud: {}", baud, e);
                continue;
            }
        };

        // Flush any stale data.
        let _ = transport.port.clear(serialport::ClearBuffer::All);

        // Send the command.
        if let Err(e) = Transport::write_all(&mut transport, &cmd_bytes) {
            warn!("write failed at {} baud: {}", baud, e);
            continue;
        }

        // Wait a bit and try to read a response.
        let mut buf = [0u8; 64];
        let mut accumulated = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_millis(1000);

        while std::time::Instant::now() < deadline {
            match Transport::read(&mut transport, &mut buf) {
                Ok(n) if n > 0 => {
                    accumulated.extend_from_slice(&buf[..n]);
                    // Check if we have a complete frame (not the echo).
                    if let Ok(Some(_)) = find_response_frame(&accumulated) {
                        info!("auto-detected {} baud on {}", baud, port_name);
                        return Ok((baud, transport));
                    }
                }
                _ => {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }

        debug!("no response at {} baud", baud);
    }

    Err(CivError::Timeout)
}

/// Search the buffer for a response frame (one that isn't the echo).
/// A response frame is one addressed to the controller (dst = E0).
fn find_response_frame(buf: &[u8]) -> Result<Option<Frame>> {
    let mut offset = 0;
    while offset < buf.len() {
        // Look for preamble.
        let remaining = &buf[offset..];
        let start = remaining
            .windows(2)
            .position(|w| w[0] == PREAMBLE && w[1] == PREAMBLE);
        let start = match start {
            Some(s) => s,
            None => return Ok(None),
        };

        match Frame::parse(&remaining[start..])? {
            Some((frame, consumed)) => {
                offset += start + consumed;
                // Skip echo frames (dst = radio address).
                if frame.dst == crate::protocol::ADDR_CONTROLLER {
                    return Ok(Some(frame));
                }
                // Otherwise it's an echo; keep looking.
            }
            None => return Ok(None),
        }
    }
    Ok(None)
}
