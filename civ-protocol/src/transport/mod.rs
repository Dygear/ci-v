use std::io;
use std::time::Duration;

#[cfg(feature = "serial")]
pub mod serial;

/// A byte-oriented transport for CI-V communication.
///
/// Implementors provide read/write access to a serial-like connection.
/// The transport is synchronous and blocking.
pub trait Transport: Send {
    /// Write all bytes to the transport.
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;

    /// Flush any buffered output.
    fn flush(&mut self) -> io::Result<()>;

    /// Read bytes into the buffer. Returns the number of bytes read.
    /// Should return `Ok(0)` or `Err(TimedOut)` on timeout, not block forever.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// Set the read timeout for subsequent `read()` calls.
    fn set_read_timeout(&mut self, timeout: Duration) -> io::Result<()>;
}
