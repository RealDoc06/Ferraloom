use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tracing::info;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use pumpkin_nbt::{NbtCompound, tag::NbtTag};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use serde::{Deserialize, Serialize};

use crate::chunk::{palette::BlockPalette, poi_changes::PoiSectionChanges};
use crate::chunk_system::ChunkPos;
use crate::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION as DATA_VERSION;
use pumpkin_data::poi::PoiType;

/// POI type identifier for nether portals
pub const POI_TYPE_NETHER_PORTAL: &str = "minecraft:nether_portal";

/// MCA format constants
const SECTOR_SIZE: usize = 4096;
const REGION_SIZE: usize = 32;
const CHUNK_COUNT: usize = REGION_SIZE * REGION_SIZE;
const HEADER_SIZE: usize = SECTOR_SIZE * 2; // Location table + timestamp table

/// Compression type for MCA format
const COMPRESSION_ZLIB: u8 = 2;

// The previous private writer stamped 3955 on records with separate x/y/z Ints.
// This is a bounded repair of that format, not support for vanilla 1.21 or DFU.
const LEGACY_XYZ_DATA_VERSION: i32 = 3955;

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The typed codec has no raw remainder storage. Refuse extensions before a
/// loaded region can be published or rewritten with those fields missing.
fn reject_extensions(compound: &NbtCompound, allowed: &[&str], scope: &str) -> io::Result<()> {
    for field in compound.child_tags.keys() {
        if !allowed.contains(&field.as_ref()) {
            return Err(invalid_data(format!(
                "Unsupported POI {scope} field {field}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum PoiReconcileError {
    #[error("POI region {x},{z} must be preloaded before taking a terrain guard")]
    RegionNotLoaded { x: i32, z: i32 },
    #[error("POI region is unavailable: {0}")]
    RegionUnavailable(#[source] io::Error),
    #[error("Loaded POI handle belongs to a different chunk")]
    WrongChunk,
    #[error("POI tracker belongs to a different storage instance")]
    WrongStorage,
    #[error("POI storage or publication lock is poisoned")]
    OwnershipPoisoned,
    #[error("POI preparation belongs to an obsolete chunk publication")]
    ObsoletePublication,
    #[error("Runtime chunk has no POI storage binding")]
    UnboundChunk,
    #[error("Incomplete Proto terrain cannot produce a FULL POI preparation stamp")]
    IncompleteTerrain,
    #[error("POI preparation worker failed: {0}")]
    PreparationWorkerFailed(String),
    #[error("POI terrain coordinates or section layout are invalid")]
    InvalidCoordinates,
    #[error("POI section revision exhausted; chunk preparation is unavailable")]
    RevisionExhausted,
    #[error("Terrain changed after save capture; retry without acknowledging POI history")]
    StaleSaveCapture,
    #[error("POI change metadata lock is poisoned; chunk preparation is unavailable")]
    MetadataPoisoned,
    #[error("Terrain lock is poisoned; POI preparation is unavailable")]
    TerrainPoisoned,
    #[error("Cannot reconcile unknown POI type {identifier} at {pos:?}")]
    UnknownType { identifier: String, pos: BlockPos },
}

/// A narrow exclusive borrow of already-loaded memory. It cannot load or save.
/// Keep the owning `PoiStorage` mutex held from ticket lookup through replacement.
pub struct LoadedPoiRegion<'a> {
    chunk: ChunkPos,
    region: &'a mut PoiRegion,
    pub(crate) identity: std::sync::Arc<()>,
}

/// Private, validated replacement. Save capture keeps its loaded-region borrow
/// until application, so ticket lookup cannot race another storage operation.
pub(crate) struct PoiSectionReplacement {
    section_y: i32,
    origin: (i32, i32, i32),
    entries: Vec<PoiEntry>,
    complete: bool,
}

impl LoadedPoiRegion<'_> {
    pub(crate) fn check_chunk(&self, chunk: ChunkPos) -> Result<(), PoiReconcileError> {
        if self.chunk == chunk {
            Ok(())
        } else {
            Err(PoiReconcileError::WrongChunk)
        }
    }

    /// Complete, bounded section replacement under the caller's terrain guard.
    /// Unknown typed records remain usable by the existing storage API, but a
    /// section containing one cannot be safely declared reconciled with terrain.
    pub(crate) fn reconcile_section(
        &mut self,
        section_y: i32,
        palette: &BlockPalette,
        changes: &PoiSectionChanges,
        complete: bool,
    ) -> Result<(), PoiReconcileError> {
        let replacement = self.plan_section(section_y, palette, changes, complete)?;
        self.apply_section(replacement);
        Ok(())
    }

    pub(crate) fn plan_section(
        &self,
        section_y: i32,
        palette: &BlockPalette,
        changes: &PoiSectionChanges,
        complete: bool,
    ) -> Result<PoiSectionReplacement, PoiReconcileError> {
        let origin = (
            self.chunk.x.checked_mul(16),
            section_y.checked_mul(16),
            self.chunk.y.checked_mul(16),
        );
        let (Some(x), Some(y), Some(z)) = origin else {
            return Err(PoiReconcileError::InvalidCoordinates);
        };
        // Multiples of 16 in i32 leave room for all local coordinates 0..15.
        let position = |index: usize| {
            (
                x + (index % 16) as i32,
                y + (index / 256) as i32,
                z + (index / 16 % 16) as i32,
            )
        };
        let mut replacement = Vec::new();
        for (index, state) in palette.iter().enumerate() {
            let key = position(index);
            let old = self.region.entries.get(&key);
            if let Some(old) = old
                && !PoiType::ALL
                    .iter()
                    .any(|kind| kind.identifier() == old.poi_type)
            {
                return Err(PoiReconcileError::UnknownType {
                    identifier: old.poi_type.clone(),
                    pos: old.pos(),
                });
            }
            if let Some(kind) = PoiType::from_state(state) {
                let free_tickets = old
                    .filter(|old| {
                        old.poi_type == kind.identifier() && !changes.resets_tickets(index)
                    })
                    .map_or(kind.max_tickets(), |old| old.free_tickets);
                replacement.push(PoiEntry {
                    x: key.0,
                    y: key.1,
                    z: key.2,
                    poi_type: kind.identifier().to_owned(),
                    free_tickets,
                });
            }
        }
        Ok(PoiSectionReplacement {
            section_y,
            origin: (x, y, z),
            entries: replacement,
            complete,
        })
    }

    /// No validation or IO remains after planning. The caller still owns the
    /// same exclusive storage borrow used to read the original tickets.
    pub(crate) fn apply_section(&mut self, replacement: PoiSectionReplacement) {
        let (x, y, z) = replacement.origin;
        for index in 0..BlockPalette::VOLUME {
            self.region.entries.remove(&(
                x + (index % 16) as i32,
                y + (index / 256) as i32,
                z + (index / 16 % 16) as i32,
            ));
        }
        for entry in replacement.entries {
            self.region
                .entries
                .insert((entry.x, entry.y, entry.z), entry);
        }
        self.region.section_validity.insert(
            (self.chunk.x, self.chunk.y, replacement.section_y),
            i8::from(replacement.complete),
        );
        self.region
            .dirty_chunks
            .insert((self.chunk.x, self.chunk.y));
        self.region.dirty = true;
    }
}

fn region_coords_from_path(path: &Path) -> io::Result<(i32, i32)> {
    let parts: Vec<_> = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .split('.')
        .collect();
    if let ["r", x, z, "mca"] = parts.as_slice()
        && let (Ok(x), Ok(z)) = (x.parse(), z.parse())
    {
        return Ok((x, z));
    }
    Err(invalid_data(format!(
        "Invalid POI region path: {}",
        path.display()
    )))
}

/// A single Point of Interest entry (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoiEntry {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    #[serde(rename = "type")]
    pub poi_type: String,
    pub free_tickets: i32,
}

impl PoiEntry {
    /// Accept the current typed position or the exact previous private layout.
    /// Never fall back from a malformed `pos`, combine layouts, or invent axes.
    fn from_nbt(record: &NbtCompound, data_version: i32) -> io::Result<Self> {
        let fields: &[&str] = if data_version == LEGACY_XYZ_DATA_VERSION {
            &["x", "y", "z", "type", "free_tickets"]
        } else {
            &["pos", "type", "free_tickets"]
        };
        reject_extensions(record, fields, "record")?;
        let has_xyz = ["x", "y", "z"]
            .iter()
            .any(|axis| record.get(axis).is_some());
        let [x, y, z] = match (data_version, record.get("pos"), has_xyz) {
            (DATA_VERSION, Some(NbtTag::IntArray(pos)), false) => pos
                .as_slice()
                .try_into()
                .map_err(|_| invalid_data("POI pos must contain exactly three Ints"))?,
            (LEGACY_XYZ_DATA_VERSION, None, true) => {
                let axis = |name| {
                    record
                        .get_int(name)
                        .ok_or_else(|| invalid_data(format!("Legacy POI requires an Int {name}")))
                };
                [axis("x")?, axis("y")?, axis("z")?]
            }
            _ => return Err(invalid_data("Unsupported or malformed POI position layout")),
        };
        let poi_type = record
            .get_string("type")
            .ok_or_else(|| invalid_data("POI requires a String type"))?
            .to_owned();
        let free_tickets = match record.get("free_tickets") {
            None => 0,
            Some(NbtTag::Int(value)) => *value,
            Some(_) => return Err(invalid_data("POI free_tickets must be an Int")),
        };
        Ok(Self {
            x,
            y,
            z,
            poi_type,
            free_tickets,
        })
    }

    #[must_use]
    pub fn new_portal(pos: BlockPos) -> Self {
        Self {
            x: pos.0.x,
            y: pos.0.y,
            z: pos.0.z,
            poi_type: POI_TYPE_NETHER_PORTAL.to_string(),
            free_tickets: 0,
        }
    }

    #[must_use]
    pub const fn pos(&self) -> BlockPos {
        BlockPos(Vector3::new(self.x, self.y, self.z))
    }
}

/// Runtime section data; the MCA codec supplies the vanilla record layout.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoiSectionData {
    #[serde(default)]
    pub valid: i8,
    #[serde(default)]
    pub records: Vec<PoiEntry>,
}

/// Runtime chunk data; persistence uses the explicit typed NBT codec below.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoiChunkData {
    pub data_version: i32,
    /// Sections keyed by Y section coordinate (e.g., "-1", "0", "1", "4")
    pub sections: FxHashMap<String, PoiSectionData>,
}

/// POI data for a single region (32x32 chunks) using MCA format
#[derive(Debug, Default)]
pub struct PoiRegion {
    /// Entries indexed by position
    entries: FxHashMap<(i32, i32, i32), PoiEntry>,
    /// Preserve loaded validity and empty sections when rewriting the region.
    /// Keys are absolute chunk x/z and section y coordinates.
    section_validity: FxHashMap<(i32, i32, i32), i8>,
    /// Track which chunks are dirty
    dirty_chunks: rustc_hash::FxHashSet<(i32, i32)>,
    dirty: bool,
}

impl PoiRegion {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    const fn pos_key(pos: &BlockPos) -> (i32, i32, i32) {
        (pos.0.x, pos.0.y, pos.0.z)
    }

    /// Get chunk index in MCA file (0-1023)
    const fn chunk_index(chunk_x: i32, chunk_z: i32) -> usize {
        let local_x = chunk_x & 31;
        let local_z = chunk_z & 31;
        ((local_z << 5) | local_x) as usize
    }

    /// Returns section key as just the Y section coordinate (like vanilla)
    fn section_key(pos: &BlockPos) -> String {
        let section_y = pos.0.y >> 4;
        section_y.to_string()
    }

    pub fn add(&mut self, entry: PoiEntry) {
        let chunk_x = entry.x >> 4;
        let chunk_z = entry.z >> 4;
        self.dirty_chunks.insert((chunk_x, chunk_z));
        self.section_validity
            .entry((chunk_x, chunk_z, entry.y >> 4))
            .or_insert(1);
        let key = (entry.x, entry.y, entry.z);
        self.entries.insert(key, entry);
        self.dirty = true;
    }

    pub fn remove(&mut self, pos: &BlockPos) -> bool {
        let key = Self::pos_key(pos);
        if self.entries.remove(&key).is_some() {
            let chunk_x = pos.0.x >> 4;
            let chunk_z = pos.0.z >> 4;
            self.dirty_chunks.insert((chunk_x, chunk_z));
            self.dirty = true;
            return true;
        }
        false
    }

    #[must_use]
    pub fn get_all(&self) -> Vec<&PoiEntry> {
        self.entries.values().collect()
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
        self.dirty_chunks.clear();
    }

    /// Group entries by chunk, then create chunk NBT data
    fn get_chunk_data(&self, chunk_x: i32, chunk_z: i32) -> Option<PoiChunkData> {
        let mut sections: FxHashMap<String, PoiSectionData> = FxHashMap::default();
        for (&(x, z, y), &valid) in &self.section_validity {
            if (x, z) == (chunk_x, chunk_z) {
                sections.insert(
                    y.to_string(),
                    PoiSectionData {
                        valid,
                        records: Vec::new(),
                    },
                );
            }
        }

        for entry in self.entries.values() {
            let entry_chunk_x = entry.x >> 4;
            let entry_chunk_z = entry.z >> 4;

            if entry_chunk_x != chunk_x || entry_chunk_z != chunk_z {
                continue;
            }

            let section_key = Self::section_key(&entry.pos());
            let section = sections
                .entry(section_key)
                .or_insert_with(|| PoiSectionData {
                    valid: 1,
                    records: Vec::new(),
                });
            section.records.push(entry.clone());
        }

        if sections.is_empty() {
            None
        } else {
            Some(PoiChunkData {
                data_version: DATA_VERSION,
                sections,
            })
        }
    }

    /// Compress chunk data to bytes
    fn compress_chunk_data(chunk_data: &PoiChunkData) -> std::io::Result<Vec<u8>> {
        let mut root = pumpkin_nbt::compound::NbtCompound::new();
        root.put_int("DataVersion", chunk_data.data_version);

        let mut sections_comp = pumpkin_nbt::compound::NbtCompound::new();
        for (sec_key, sec_data) in &chunk_data.sections {
            let mut sec_comp = pumpkin_nbt::compound::NbtCompound::new();
            sec_comp.put_byte("Valid", sec_data.valid);
            let mut rec_list = Vec::new();
            for rec in &sec_data.records {
                let mut rec_comp = pumpkin_nbt::compound::NbtCompound::new();
                rec_comp.put("pos", NbtTag::IntArray(vec![rec.x, rec.y, rec.z]));
                rec_comp.put_string("type", rec.poi_type.clone());
                rec_comp.put_int("free_tickets", rec.free_tickets);
                rec_list.push(pumpkin_nbt::tag::NbtTag::Compound(rec_comp));
            }
            sec_comp.put_list("Records", rec_list);
            sections_comp.put_compound(sec_key, sec_comp);
        }
        root.put_compound("Sections", sections_comp);

        let uncompressed = crate::nbt_io::encode_compound(root)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&uncompressed)?;
        encoder.finish()
    }

    /// Decompress chunk data from bytes
    fn decompress_chunk_data(compressed: &[u8]) -> std::io::Result<PoiChunkData> {
        let mut decoder = ZlibDecoder::new(compressed);
        let mut uncompressed = Vec::new();
        decoder.read_to_end(&mut uncompressed)?;

        let mut cursor = Cursor::new(uncompressed);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
        );
        let nbt = pumpkin_nbt::Nbt::read(&mut reader)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        if cursor.position() != cursor.get_ref().len() as u64 {
            return Err(invalid_data("Trailing POI NBT data"));
        }
        let data_version = nbt
            .get_int("DataVersion")
            .ok_or_else(|| invalid_data("POI requires an Int DataVersion"))?;
        if !matches!(data_version, DATA_VERSION | LEGACY_XYZ_DATA_VERSION) {
            return Err(invalid_data(format!(
                "Unsupported POI DataVersion {data_version}; no DFU is applied"
            )));
        }
        reject_extensions(&nbt.root_tag, &["DataVersion", "Sections"], "root")?;
        let mut sections = FxHashMap::default();
        let sec_tag = nbt
            .get_compound("Sections")
            .ok_or_else(|| invalid_data("POI requires a Sections compound"))?;
        for (sec_key, tag) in &sec_tag.child_tags {
            let section_y = sec_key
                .parse::<i32>()
                .map_err(|_| invalid_data("Invalid POI section coordinate"))?;
            if section_y.to_string() != sec_key.as_ref() {
                return Err(invalid_data("Noncanonical POI section coordinate"));
            }
            let NbtTag::Compound(sec_comp) = tag else {
                return Err(invalid_data("POI section must be a compound"));
            };
            reject_extensions(sec_comp, &["Valid", "Records"], "section")?;
            // The official codec defaults absent Valid to false. Reject malformed
            // tags rather than claiming equivalence to its lenient decoding.
            let valid = match sec_comp.get("Valid") {
                None => 0,
                Some(NbtTag::Byte(value @ (0 | 1))) => *value,
                Some(_) => return Err(invalid_data("POI Valid must be a boolean Byte")),
            };
            let rec_list = sec_comp
                .get_list("Records")
                .ok_or_else(|| invalid_data("POI requires a Records list"))?;
            let mut records = Vec::with_capacity(rec_list.len());
            for tag in rec_list {
                let NbtTag::Compound(record) = tag else {
                    return Err(invalid_data("POI record must be a compound"));
                };
                let entry = PoiEntry::from_nbt(record, data_version)?;
                if entry.y >> 4 != section_y {
                    return Err(invalid_data("POI position does not belong to its section"));
                }
                records.push(entry);
            }
            sections.insert(sec_key.to_string(), PoiSectionData { valid, records });
        }

        Ok(PoiChunkData {
            data_version,
            sections,
        })
    }

    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        if self.entries.is_empty() && self.section_validity.is_empty() {
            // Don't save empty regions, delete the file if it exists
            if path.exists() {
                std::fs::remove_file(path)?;
            }
            self.dirty = false;
            self.dirty_chunks.clear();
            return Ok(());
        }

        // Build all chunk data
        let mut chunk_data_map: FxHashMap<usize, Vec<u8>> = FxHashMap::default();

        let region_coords = region_coords_from_path(path)?;
        // Include empty loaded sections, which still carry validity information.
        let mut chunks_with_data: rustc_hash::FxHashSet<(i32, i32)> =
            rustc_hash::FxHashSet::default();
        for &(x, z, _) in self.section_validity.keys() {
            chunks_with_data.insert((x, z));
        }
        for entry in self.entries.values() {
            chunks_with_data.insert((entry.x >> 4, entry.z >> 4));
        }

        for (chunk_x, chunk_z) in &chunks_with_data {
            if (*chunk_x >> 5, *chunk_z >> 5) != region_coords {
                return Err(invalid_data(
                    "POI position is outside the destination region",
                ));
            }
            if let Some(chunk_data) = self.get_chunk_data(*chunk_x, *chunk_z) {
                let compressed = Self::compress_chunk_data(&chunk_data)?;
                let index = Self::chunk_index(*chunk_x, *chunk_z);
                chunk_data_map.insert(index, compressed);
            }
        }

        // Build MCA file
        let mut location_table = [0u32; CHUNK_COUNT];
        let mut timestamp_table = [0u32; CHUNK_COUNT];
        let mut sector_data: Vec<Vec<u8>> = Vec::new();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as u32);

        // Start after header (2 sectors)
        let mut current_sector: u32 = 2;

        for index in 0..CHUNK_COUNT {
            if let Some(compressed) = chunk_data_map.get(&index) {
                // Calculate sector count needed
                let data_len = compressed.len() + 5; // 4 bytes length + 1 byte compression + data
                let sector_count = data_len.div_ceil(SECTOR_SIZE) as u32;
                if sector_count > u32::from(u8::MAX) {
                    return Err(invalid_data(
                        "POI chunk exceeds the inline MCA sector limit",
                    ));
                }

                // Build padded sector data
                let mut padded = Vec::with_capacity(sector_count as usize * SECTOR_SIZE);
                let length = (compressed.len() + 1) as u32; // +1 for compression byte
                padded.extend_from_slice(&length.to_be_bytes());
                padded.push(COMPRESSION_ZLIB);
                padded.extend_from_slice(compressed);
                // Pad to sector boundary
                padded.resize(sector_count as usize * SECTOR_SIZE, 0);

                location_table[index] = (current_sector << 8) | sector_count;
                timestamp_table[index] = timestamp;
                sector_data.push(padded);

                current_sector += sector_count;
            }
        }

        // Write file
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        crate::nbt_io::write_atomic_with(path, |file| {
            for loc in &location_table {
                file.write_all(&loc.to_be_bytes())?;
            }
            for ts in &timestamp_table {
                file.write_all(&ts.to_be_bytes())?;
            }
            for data in &sector_data {
                file.write_all(data)?;
            }
            Ok(())
        })?;

        self.dirty = false;
        self.dirty_chunks.clear();
        Ok(())
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let file_data = match std::fs::read(path) {
            Ok(data) => data,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::new()),
            Err(error) => return Err(error),
        };
        if file_data.len() < HEADER_SIZE {
            return Err(invalid_data("Truncated POI region header"));
        }

        let region_coords = region_coords_from_path(path)?;
        let mut region = Self::new();
        let mut occupied_sectors = rustc_hash::FxHashSet::default();

        // Parse location table
        for index in 0..CHUNK_COUNT {
            let offset = index * 4;
            let location = u32::from_be_bytes([
                file_data[offset],
                file_data[offset + 1],
                file_data[offset + 2],
                file_data[offset + 3],
            ]);

            let sector_offset = (location >> 8) as usize;
            let sector_count = (location & 0xFF) as usize;

            if location == 0 {
                continue;
            }
            if sector_offset < 2 || sector_count == 0 {
                return Err(invalid_data(format!(
                    "Invalid POI allocation at chunk {index}"
                )));
            }

            let byte_offset = sector_offset * SECTOR_SIZE;
            let byte_end = byte_offset + sector_count * SECTOR_SIZE;

            if byte_end > file_data.len() {
                return Err(invalid_data(format!(
                    "Truncated POI allocation at chunk {index}"
                )));
            }
            for sector in sector_offset..sector_offset + sector_count {
                if !occupied_sectors.insert(sector) {
                    return Err(invalid_data("Overlapping POI chunk allocations"));
                }
            }

            // Read chunk data
            let chunk_bytes = &file_data[byte_offset..byte_end];
            let length = u32::from_be_bytes([
                chunk_bytes[0],
                chunk_bytes[1],
                chunk_bytes[2],
                chunk_bytes[3],
            ]) as usize;
            let compression = chunk_bytes[4];

            if compression != COMPRESSION_ZLIB {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("Unsupported POI compression {compression} at chunk {index}"),
                ));
            }
            if length <= 1 || length > chunk_bytes.len() - 4 {
                return Err(invalid_data(format!("Invalid POI length at chunk {index}")));
            }

            let compressed = &chunk_bytes[5..5 + length - 1];

            let chunk_data = Self::decompress_chunk_data(compressed)
                .map_err(|error| invalid_data(format!("Invalid POI chunk {index}: {error}")))?;
            // Re-encode the recognized private layout on the next successful save.
            region.dirty |= chunk_data.data_version == LEGACY_XYZ_DATA_VERSION;
            let chunk_x = i64::from(region_coords.0) * 32 + (index % REGION_SIZE) as i64;
            let chunk_z = i64::from(region_coords.1) * 32 + (index / REGION_SIZE) as i64;
            let (chunk_x, chunk_z) = (
                i32::try_from(chunk_x).map_err(|_| invalid_data("POI chunk x overflow"))?,
                i32::try_from(chunk_z).map_err(|_| invalid_data("POI chunk z overflow"))?,
            );
            if chunk_data.data_version == LEGACY_XYZ_DATA_VERSION {
                region.dirty_chunks.insert((chunk_x, chunk_z));
            }
            for (section_key, section) in chunk_data.sections {
                let section_y = section_key
                    .parse::<i32>()
                    .map_err(|_| invalid_data("Invalid POI section coordinate"))?;
                region
                    .section_validity
                    .insert((chunk_x, chunk_z, section_y), section.valid);
                for entry in section.records {
                    if (entry.x >> 4, entry.z >> 4) != (chunk_x, chunk_z) {
                        return Err(invalid_data(
                            "POI position does not belong to its MCA chunk",
                        ));
                    }
                    let key = (entry.x, entry.y, entry.z);
                    if region.entries.insert(key, entry).is_some() {
                        return Err(invalid_data("Duplicate POI position"));
                    }
                }
            }
        }

        Ok(region)
    }
}

/// Region-based POI storage using MCA format
pub struct PoiStorage {
    identity: std::sync::Arc<()>,
    /// Path to the poi folder
    folder: PathBuf,
    /// Loaded regions, keyed by (`region_x`, `region_z`)
    regions: FxHashMap<(i32, i32), PoiRegion>,
    /// Failed regions stay unavailable until storage is reopened. Never retry
    /// them as empty after a subsequent read or mutation request.
    load_errors: FxHashMap<(i32, i32), io::Error>,
}

impl PoiStorage {
    #[must_use]
    pub fn new(poi_folder: PathBuf) -> Self {
        Self {
            identity: std::sync::Arc::new(()),
            folder: poi_folder,
            regions: FxHashMap::default(),
            load_errors: FxHashMap::default(),
        }
    }

    const fn region_coords(pos: &BlockPos) -> (i32, i32) {
        let chunk_x = pos.0.x >> 4;
        let chunk_z = pos.0.z >> 4;
        (chunk_x >> 5, chunk_z >> 5)
    }

    fn region_path(&self, rx: i32, rz: i32) -> PathBuf {
        self.folder.join(format!("r.{rx}.{rz}.mca"))
    }

    fn get_or_load_region(&mut self, rx: i32, rz: i32) -> io::Result<&mut PoiRegion> {
        if let Some(error) = self.load_errors.get(&(rx, rz)) {
            return Err(io::Error::new(error.kind(), error.to_string()));
        }
        let path = self.region_path(rx, rz);
        match self.regions.entry((rx, rz)) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => match PoiRegion::load(&path) {
                Ok(region) => Ok(entry.insert(region)),
                Err(error) => {
                    let message = format!("Failed to load POI region {}: {error}", path.display());
                    self.load_errors
                        .insert((rx, rz), io::Error::new(error.kind(), message.clone()));
                    Err(io::Error::new(error.kind(), message))
                }
            },
        }
    }

    /// IO phase: hold only the shared storage mutex, with no terrain/lifecycle
    /// guard. Failures retain the existing sticky unavailable-region behavior.
    pub fn preload_poi_region(&mut self, chunk: ChunkPos) -> io::Result<()> {
        self.get_or_load_region(chunk.x >> 5, chunk.y >> 5)
            .map(|_| ())
    }

    /// Memory phase: never falls back to loading. Keep the storage mutex through
    /// the terrain read guard and reconciliation so ticket changes cannot race.
    pub fn loaded_poi_region(
        &mut self,
        chunk: ChunkPos,
    ) -> Result<LoadedPoiRegion<'_>, PoiReconcileError> {
        let coords = (chunk.x >> 5, chunk.y >> 5);
        if let Some(error) = self.load_errors.get(&coords) {
            return Err(PoiReconcileError::RegionUnavailable(io::Error::new(
                error.kind(),
                error.to_string(),
            )));
        }
        let region = self
            .regions
            .get_mut(&coords)
            .ok_or(PoiReconcileError::RegionNotLoaded {
                x: coords.0,
                z: coords.1,
            })?;
        Ok(LoadedPoiRegion {
            chunk,
            region,
            identity: self.identity.clone(),
        })
    }

    pub fn add(&mut self, pos: BlockPos, poi_type: &str) -> io::Result<()> {
        self.add_with_free_tickets(pos, poi_type, 0)
    }

    pub fn add_with_free_tickets(
        &mut self,
        pos: BlockPos,
        poi_type: &str,
        free_tickets: i32,
    ) -> io::Result<()> {
        let (rx, rz) = Self::region_coords(&pos);
        let region = self.get_or_load_region(rx, rz)?;
        region.add(PoiEntry {
            x: pos.0.x,
            y: pos.0.y,
            z: pos.0.z,
            poi_type: poi_type.to_string(),
            free_tickets,
        });
        Ok(())
    }

    pub fn add_portal(&mut self, pos: BlockPos) -> io::Result<()> {
        self.add(pos, POI_TYPE_NETHER_PORTAL)
    }

    pub fn remove(&mut self, pos: &BlockPos) -> io::Result<bool> {
        let (rx, rz) = Self::region_coords(pos);
        let region = self.get_or_load_region(rx, rz)?;
        Ok(region.remove(pos))
    }

    /// Get all POI positions within a square radius (for portal search)
    #[expect(clippy::similar_names)]
    pub fn get_in_square(
        &mut self,
        center: BlockPos,
        radius: i32,
        poi_type: Option<&str>,
    ) -> io::Result<Vec<BlockPos>> {
        let min_x = center.0.x - radius;
        let max_x = center.0.x + radius;
        let min_z = center.0.z - radius;
        let max_z = center.0.z + radius;

        // Calculate which regions we need to check
        let min_rx = (min_x >> 4) >> 5;
        let max_rx = (max_x >> 4) >> 5;
        let min_rz = (min_z >> 4) >> 5;
        let max_rz = (max_z >> 4) >> 5;

        let mut results = Vec::new();

        for rx in min_rx..=max_rx {
            for rz in min_rz..=max_rz {
                let region = self.get_or_load_region(rx, rz)?;
                for entry in region.get_all() {
                    if let Some(filter_type) = poi_type
                        && entry.poi_type != filter_type
                    {
                        continue;
                    }

                    let dx = (entry.x - center.0.x).abs();
                    let dz = (entry.z - center.0.z).abs();
                    if dx <= radius && dz <= radius {
                        results.push(entry.pos());
                    }
                }
            }
        }

        Ok(results)
    }

    /// Finds the closest POI whose type matches `matches`, considering
    /// entries within `radius` blocks of `center` on the x/z axes (like
    /// vanilla's `PoiManager.findClosestWithType`: a chebyshev square gather
    /// followed by picking the smallest 3D squared distance).
    ///
    /// Returns the entry's position together with its type.
    pub fn find_closest_matching(
        &mut self,
        center: BlockPos,
        radius: i32,
        matches: impl Fn(&str) -> bool,
    ) -> io::Result<Option<(BlockPos, String)>> {
        let min_rx = ((center.0.x - radius) >> 4) >> 5;
        let max_rx = ((center.0.x + radius) >> 4) >> 5;
        let min_rz = ((center.0.z - radius) >> 4) >> 5;
        let max_rz = ((center.0.z + radius) >> 4) >> 5;

        let mut best: Option<(BlockPos, String, i64)> = None;

        for rx in min_rx..=max_rx {
            for rz in min_rz..=max_rz {
                let region = self.get_or_load_region(rx, rz)?;
                for entry in region.get_all() {
                    if (entry.x - center.0.x).abs() > radius
                        || (entry.z - center.0.z).abs() > radius
                        || !matches(&entry.poi_type)
                    {
                        continue;
                    }

                    let dx = i64::from(entry.x - center.0.x);
                    let dy = i64::from(entry.y - center.0.y);
                    let dz = i64::from(entry.z - center.0.z);
                    let distance_sq = dx * dx + dy * dy + dz * dz;

                    if best.as_ref().is_none_or(|(_, _, d)| distance_sq < *d) {
                        best = Some((entry.pos(), entry.poi_type.clone(), distance_sq));
                    }
                }
            }
        }

        Ok(best.map(|(pos, poi_type, _)| (pos, poi_type)))
    }

    /// Flush only the prepared region, after terrain encoding and after releasing
    /// terrain/publication guards. Other unavailable regions do not block it.
    pub(crate) fn save_prepared_region(&mut self, chunk: ChunkPos) -> io::Result<()> {
        let path = self.region_path(chunk.x >> 5, chunk.y >> 5);
        std::fs::create_dir_all(&self.folder)?;
        let loaded = self.loaded_poi_region(chunk).map_err(io::Error::other)?;
        if loaded.region.is_dirty() {
            loaded.region.save(&path)?;
        }
        Ok(())
    }

    pub fn save_all(&mut self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.folder)?;

        let mut failure = self
            .load_errors
            .values()
            .next()
            .map(|error| io::Error::new(error.kind(), error.to_string()));
        let mut saved = 0;
        for ((rx, rz), region) in &mut self.regions {
            if region.is_dirty() {
                let path = self.folder.join(format!("r.{rx}.{rz}.mca"));
                match region.save(&path) {
                    Ok(()) => saved += 1,
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                }
            }
        }

        if saved > 0 {
            info!("Saved {saved} POI region(s)");
        }
        // Healthy loaded regions can persist, but the overall save must still
        // report a failed lazy load. No failed region was inserted into the map.
        failure.map_or(Ok(()), Err)
    }

    /// Get count of loaded regions
    #[must_use]
    pub fn loaded_region_count(&self) -> usize {
        self.regions.len()
    }

    /// Get total POI count across all loaded regions
    #[must_use]
    pub fn total_poi_count(&self) -> usize {
        self.regions.values().map(|r| r.get_all().len()).sum()
    }
}

#[cfg(test)]
mod persistence_tests;

#[cfg(test)]
mod reconciliation_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_nbt_poi_encode_failure_keeps_previous_region_and_dirty_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("r.0.0.mca");
        let mut region = PoiRegion::new();
        let position = BlockPos::new(0, 64, 0);
        region.add(PoiEntry::new_portal(position));
        region.save(&path).unwrap();
        let previous = std::fs::read(&path).unwrap();
        let mut oversized = PoiEntry::new_portal(position);
        oversized.poi_type = "😀".repeat(10_923);
        region.add(oversized);
        assert_eq!(
            region.save(&path).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        assert!(region.is_dirty());
        assert!(region.dirty_chunks.contains(&(0, 0)));

        region.add(PoiEntry::new_portal(position));
        region.save(&path).unwrap();
        assert!(!region.is_dirty());
        assert!(region.dirty_chunks.is_empty());
        let reloaded = PoiRegion::load(&path).unwrap();
        assert_eq!(reloaded.get_all()[0].poi_type, POI_TYPE_NETHER_PORTAL);
    }

    #[test]
    fn poi_entry() {
        let entry = PoiEntry::new_portal(BlockPos(Vector3::new(100, 64, 200)));
        assert_eq!(entry.x, 100);
        assert_eq!(entry.y, 64);
        assert_eq!(entry.z, 200);
        assert_eq!(entry.poi_type, POI_TYPE_NETHER_PORTAL);
    }

    #[test]
    fn poi_region() {
        let mut region = PoiRegion::new();
        region.add(PoiEntry::new_portal(BlockPos(Vector3::new(100, 64, 200))));
        region.add(PoiEntry::new_portal(BlockPos(Vector3::new(101, 64, 200))));

        assert_eq!(region.get_all().len(), 2);
        assert!(region.is_dirty());

        region.remove(&BlockPos(Vector3::new(100, 64, 200)));
        assert_eq!(region.get_all().len(), 1);
    }

    #[test]
    fn poi_find_closest_matching() {
        let mut storage = PoiStorage::new(std::env::temp_dir().join("pumpkin_poi_closest_test"));

        storage
            .add_portal(BlockPos(Vector3::new(100, 64, 100)))
            .unwrap();
        storage
            .add_portal(BlockPos(Vector3::new(120, 64, 100)))
            .unwrap();
        storage
            .add(BlockPos(Vector3::new(101, 64, 100)), "minecraft:home")
            .unwrap();

        let center = BlockPos(Vector3::new(105, 64, 100));
        let (pos, poi_type) = storage
            .find_closest_matching(center, 256, |t| t == POI_TYPE_NETHER_PORTAL)
            .unwrap()
            .unwrap();
        assert_eq!(pos, BlockPos(Vector3::new(100, 64, 100)));
        assert_eq!(poi_type, POI_TYPE_NETHER_PORTAL);

        // The overall closest one ignores the type filter mismatch above.
        let (pos, poi_type) = storage
            .find_closest_matching(center, 256, |_| true)
            .unwrap()
            .unwrap();
        assert_eq!(pos, BlockPos(Vector3::new(101, 64, 100)));
        assert_eq!(poi_type, "minecraft:home");

        assert!(
            storage
                .find_closest_matching(center, 256, |t| t == "minecraft:lodestone")
                .unwrap()
                .is_none()
        );
        // Out of horizontal range.
        assert!(
            storage
                .find_closest_matching(BlockPos(Vector3::new(1000, 64, 100)), 16, |_| true)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn poi_storage_mca() {
        let dir = std::env::temp_dir().join("pumpkin_poi_mca_test");
        let _ = std::fs::remove_dir_all(&dir);

        let mut storage = PoiStorage::new(dir.join("poi"));

        storage
            .add_portal(BlockPos(Vector3::new(100, 64, 100)))
            .unwrap();
        storage
            .add_portal(BlockPos(Vector3::new(110, 64, 100)))
            .unwrap();
        storage
            .add_portal(BlockPos(Vector3::new(1000, 64, 1000)))
            .unwrap(); // Different region

        let results = storage
            .get_in_square(
                BlockPos(Vector3::new(105, 64, 100)),
                16,
                Some(POI_TYPE_NETHER_PORTAL),
            )
            .unwrap();
        assert_eq!(results.len(), 2);

        storage.save_all().unwrap();

        // Verify .mca file was created
        let mca_path = dir.join("poi").join("r.0.0.mca");
        assert!(mca_path.exists());

        // Reload and verify
        let mut storage2 = PoiStorage::new(dir.join("poi"));
        let results2 = storage2
            .get_in_square(
                BlockPos(Vector3::new(105, 64, 100)),
                16,
                Some(POI_TYPE_NETHER_PORTAL),
            )
            .unwrap();
        assert_eq!(results2.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod save_tests;
