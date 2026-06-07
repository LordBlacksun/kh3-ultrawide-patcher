//! Error type shared by all commands. Serializes to `{ kind, message }` for the
//! frontend so the UI can map error kinds to friendly copy.

use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum AppError {
    /// A required file (exe, backup, …) was not found.
    NotFound(String),
    /// The exe is locked (game running) — cannot write.
    Locked(String),
    /// A required patch site matched more than once — refuse to guess.
    AbortMultiMatch(String),
    /// Filesystem / I/O failure.
    Io(String),
    /// No usable backup exists to revert from.
    NoBackup(String),
    /// A safety guard tripped (e.g. would write the UI-boxing value).
    Danger(String),
    /// Anything else.
    Other(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(m) => write!(f, "Not found: {m}"),
            AppError::Locked(m) => write!(f, "{m}"),
            AppError::AbortMultiMatch(m) => write!(f, "{m}"),
            AppError::Io(m) => write!(f, "I/O error: {m}"),
            AppError::NoBackup(m) => write!(f, "{m}"),
            AppError::Danger(m) => write!(f, "{m}"),
            AppError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}
