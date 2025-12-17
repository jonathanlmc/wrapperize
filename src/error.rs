use std::path::PathBuf;

use anyhow::anyhow;

#[derive(Debug, thiserror::Error)]
#[error("{path}: {err}")]
pub struct IoError {
    path: PathBuf,
    err: anyhow::Error,
}

impl IoError {
    pub fn new(path: impl Into<PathBuf>, err: impl Into<IoErrorMessage>) -> Self {
        Self {
            path: path.into(),
            err: err.into().0,
        }
    }
}

pub struct IoErrorMessage(anyhow::Error);

impl From<anyhow::Error> for IoErrorMessage {
    fn from(value: anyhow::Error) -> Self {
        Self(value)
    }
}

impl From<&'static str> for IoErrorMessage {
    fn from(value: &'static str) -> Self {
        Self(anyhow!(value))
    }
}

impl From<String> for IoErrorMessage {
    fn from(value: String) -> Self {
        Self(anyhow!(value))
    }
}
