use thiserror::Error;

pub type Result<T> = std::result::Result<T, CivError>;

#[derive(Debug, Error)]
pub enum CivError {
    #[cfg(feature = "serial")]
    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ID-52A Plus serial port not found")]
    PortNotFound,

    #[error("invalid CI-V frame")]
    InvalidFrame,

    #[error("radio returned NG (command rejected)")]
    Ng,

    #[error("timeout waiting for response")]
    Timeout,

    #[error("invalid BCD data: {0:#04x}")]
    InvalidBcd(u8),

    #[error("frequency out of range: {0} Hz")]
    FrequencyOutOfRange(u64),

    #[error("unknown operating mode: {0:#04x}")]
    UnknownMode(u8),
}
