use std::fmt;

/// Crate-wide error type.
#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    ProjectNotFound(String),
    ProjectExists(String),
    MemoryNotFound {
        project_id: String,
        memory_id: String,
    },
    InvalidPolicy(String),
    InvalidTier(String),
    EmptyText,
    Embedding(String),
    Poisoned,
    UnknownTool(String),
    InvalidToolArgs(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "sqlite: {e}"),
            Error::Json(e) => write!(f, "json: {e}"),
            Error::Io(e) => write!(f, "io: {e}"),
            Error::ProjectNotFound(id) => write!(f, "project not found: {id}"),
            Error::ProjectExists(id) => write!(f, "project already exists: {id}"),
            Error::MemoryNotFound {
                project_id,
                memory_id,
            } => write!(f, "memory {memory_id} not found in project {project_id}"),
            Error::InvalidPolicy(msg) => write!(f, "invalid policy: {msg}"),
            Error::InvalidTier(t) => write!(f, "invalid tier: {t}"),
            Error::EmptyText => write!(f, "memory text must not be empty"),
            Error::Embedding(msg) => write!(f, "embedding: {msg}"),
            Error::Poisoned => write!(f, "store lock poisoned"),
            Error::UnknownTool(name) => write!(f, "unknown tool: {name}"),
            Error::InvalidToolArgs(msg) => write!(f, "invalid tool args: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Sqlite(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Error::Sqlite(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::Json(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
