//! The crate-wide error type. Hand-written rather than pulling in `thiserror`, so `core`
//! keeps `num-complex` as its only dependency.

use std::fmt;

/// Errors surfaced by a [`Source`](crate::Source) and the layers built on it.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// Hardware or USB failure, carrying the driver's message.
    Device(String),
    /// Underlying IO failure (file sources, recordings).
    Io(std::io::Error),
    /// Invalid configuration: an unsupported sample rate, frequency, or gain.
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Device(msg) => write!(f, "device error: {msg}"),
            Error::Io(err) => write!(f, "io error: {err}"),
            Error::Config(msg) => write!(f, "invalid configuration: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}
