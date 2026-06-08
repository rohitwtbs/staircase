//! Structured error types for Staircase.
//!
//! [`StaircaseError`] is the canonical error type used across every crate in the
//! workspace. Application code is encouraged to use [`anyhow`] at the top level
//! and convert into [`StaircaseError`] where a structured variant is helpful.

use thiserror::Error;

/// Convenience alias for results returning a [`StaircaseError`].
pub type Result<T> = std::result::Result<T, StaircaseError>;

/// The canonical, structured error type for the Staircase framework.
#[derive(Debug, Error)]
pub enum StaircaseError {
    /// A network/transport connection could not be established or was lost.
    #[error("connection error: {0}")]
    Connection(String),

    /// A protocol-level error (malformed frame, unsupported service, etc.).
    #[error("protocol error: {0}")]
    Protocol(String),

    /// An operation exceeded its allotted time.
    #[error("timeout: {0}")]
    Timeout(String),

    /// The configuration was missing, malformed, or invalid.
    #[error("configuration error: {0}")]
    Config(String),

    /// (De)serialization of a payload or message failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// The storage/persistence layer reported an error.
    #[error("storage error: {0}")]
    Storage(String),

    /// An I/O error occurred.
    #[error("io error: {0}")]
    Io(String),

    /// Any other error, typically bubbled up via [`anyhow`].
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl StaircaseError {
    /// Helper to build a [`StaircaseError::Connection`] from any displayable value.
    pub fn connection(msg: impl std::fmt::Display) -> Self {
        StaircaseError::Connection(msg.to_string())
    }

    /// Helper to build a [`StaircaseError::Protocol`] from any displayable value.
    pub fn protocol(msg: impl std::fmt::Display) -> Self {
        StaircaseError::Protocol(msg.to_string())
    }

    /// Helper to build a [`StaircaseError::Timeout`] from any displayable value.
    pub fn timeout(msg: impl std::fmt::Display) -> Self {
        StaircaseError::Timeout(msg.to_string())
    }

    /// Helper to build a [`StaircaseError::Config`] from any displayable value.
    pub fn config(msg: impl std::fmt::Display) -> Self {
        StaircaseError::Config(msg.to_string())
    }

    /// Helper to build a [`StaircaseError::Storage`] from any displayable value.
    pub fn storage(msg: impl std::fmt::Display) -> Self {
        StaircaseError::Storage(msg.to_string())
    }
}

impl From<std::io::Error> for StaircaseError {
    fn from(e: std::io::Error) -> Self {
        StaircaseError::Io(e.to_string())
    }
}

impl From<serde_yaml::Error> for StaircaseError {
    fn from(e: serde_yaml::Error) -> Self {
        StaircaseError::Serialization(e.to_string())
    }
}

impl From<serde_json::Error> for StaircaseError {
    fn from(e: serde_json::Error) -> Self {
        StaircaseError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_maps_to_io_variant() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: StaircaseError = io.into();
        assert!(matches!(err, StaircaseError::Io(_)));
        assert!(err.to_string().contains("io error"));
    }

    #[test]
    fn helpers_build_expected_variants() {
        assert!(matches!(
            StaircaseError::connection("x"),
            StaircaseError::Connection(_)
        ));
        assert!(matches!(
            StaircaseError::protocol("x"),
            StaircaseError::Protocol(_)
        ));
        assert!(matches!(StaircaseError::config("x"), StaircaseError::Config(_)));
    }
}
