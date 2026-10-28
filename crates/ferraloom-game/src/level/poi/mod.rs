//! Canonical point-of-interest types and section data.
mod types;
pub use types::{PoiType, PoiTypeId, PoiTypes};

mod error;
mod record;
mod section;
pub use error::PoiError;
pub use record::{Occupancy, PoiRecord};
pub use section::PoiSection;

mod manager;
pub use manager::{PoiChunkSave, PoiManager};

mod access;

#[cfg(test)]
mod tests;

mod update;
pub use update::PoiUpdate;
