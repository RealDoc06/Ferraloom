use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Nbt(ferraloom_nbt::NbtError),
    Json(serde_json::Error),
    Invalid(String),
    DataVersion { found: i32, expected: i32 },
    Closed,
}
pub type Result<T> = std::result::Result<T, Error>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Nbt(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(s) => f.write_str(s),
            Self::DataVersion { found, expected } => write!(
                f,
                "unsupported save DataVersion {found}; expected {expected} (26.3); data fixing is not implemented"
            ),
            Self::Closed => f.write_str("storage worker is closed"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Nbt(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<ferraloom_nbt::NbtError> for Error {
    fn from(e: ferraloom_nbt::NbtError) -> Self {
        Self::Nbt(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
