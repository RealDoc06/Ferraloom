use ferraloom_util::{BlockPos, ChunkPos};
use std::{fmt, sync::Arc};

/// Required POI failures retain IO causes and never manufacture an empty chunk.
#[derive(Clone, Debug)]
pub enum PoiError {
    MissingStorage,
    UnloadedChunk(ChunkPos),
    MissingRecord(BlockPos),
    InvalidData(String),
    Storage(Arc<ferraloom_storage::Error>),
}
impl PartialEq for PoiError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MissingStorage, Self::MissingStorage) => true,
            (Self::UnloadedChunk(a), Self::UnloadedChunk(b)) => a == b,
            (Self::MissingRecord(a), Self::MissingRecord(b)) => a == b,
            (Self::InvalidData(a), Self::InvalidData(b)) => a == b,
            // IO errors are owned cold causes; cloning preserves their identity
            // without inventing structural equality for opaque OS errors.
            (Self::Storage(a), Self::Storage(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
impl Eq for PoiError {}
impl fmt::Display for PoiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(source) => write!(formatter, "POI storage: {source}"),
            error => fmt::Debug::fmt(error, formatter),
        }
    }
}
impl std::error::Error for PoiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}
