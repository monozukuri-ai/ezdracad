use std::io;

/// Errors shared by inspection and subsequent DRA-CAD decoders.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DracadError {
    #[error("invalid format: {0}")]
    InvalidFormat(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
    #[error("{resource} limit exceeded: {actual} > {limit}")]
    LimitExceeded {
        resource: &'static str,
        limit: u64,
        actual: u64,
    },
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, DracadError>;
