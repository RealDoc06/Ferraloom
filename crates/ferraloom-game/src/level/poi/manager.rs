use super::{Occupancy, PoiError, PoiRecord, PoiSection, PoiTypeId};
use ferraloom_storage::PoiChunk;
use ferraloom_util::{BlockPos, ChunkPos, random::RandomSource};
use std::collections::BTreeMap;

#[derive(Default, Debug)]
struct LoadedChunk {
    sections: BTreeMap<i32, PoiSection>,
    revision: u64,
    dirty: bool,
}
/// Owned POI save; acknowledge only after this exact revision was persisted.
pub struct PoiChunkSave {
    pub revision: u64,
    pub chunk: PoiChunk,
}

/// Port of PoiManager/SectionStorage's dimension-owned canonical state. Presence
/// in chunks includes a known-absent disk read. There is no eviction in this slice.
#[derive(Debug)]
pub struct PoiManager {
    chunks: BTreeMap<ChunkPos, LoadedChunk>,
    min_section: i32,
    max_section: i32,
    next_revision: u64,
}
impl Default for PoiManager {
    fn default() -> Self {
        Self::new(-4, 19)
    }
}
impl PoiManager {
    pub fn new(min_section: i32, max_section: i32) -> Self {
        Self {
            chunks: BTreeMap::new(),
            min_section,
            max_section,
            next_revision: 0,
        }
    }
    pub fn set_section_bounds(&mut self, min_section: i32, max_section: i32) {
        self.min_section = min_section;
        self.max_section = max_section;
    }
    pub fn is_chunk_loaded(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }
    pub fn complete_chunk_load(
        &mut self,
        pos: ChunkPos,
        data: Option<PoiChunk>,
    ) -> Result<(), PoiError> {
        // A late prefetch must never replace a nested ensure's result/mutations.
        if self.is_chunk_loaded(pos) {
            return Ok(());
        }
        let mut loaded = LoadedChunk::default();
        if let Some(data) = data {
            for (key, packed) in data.sections.unwrap_or_default() {
                let y: i32 = key
                    .to_string_checked()
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| PoiError::InvalidData("invalid POI section Y".into()))?;
                if (self.min_section..=self.max_section).contains(&y) {
                    loaded.sections.insert(y, PoiSection::unpack(packed)?);
                }
            }
        }
        self.chunks.insert(pos, loaded);
        Ok(())
    }
    fn dirty(&mut self, pos: ChunkPos) {
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .expect("POI revision exhausted");
        let chunk = self.chunks.get_mut(&pos).expect("loaded POI chunk");
        chunk.dirty = true;
        chunk.revision = self.next_revision;
    }
    fn check_y(&self, pos: BlockPos) -> Result<(), PoiError> {
        if !(self.min_section..=self.max_section).contains(&(pos.y >> 4)) {
            Err(PoiError::InvalidData(format!(
                "POI section out of bounds at {pos:?}"
            )))
        } else {
            Ok(())
        }
    }
    fn chunk(&self, pos: ChunkPos) -> Result<&LoadedChunk, PoiError> {
        self.chunks.get(&pos).ok_or(PoiError::UnloadedChunk(pos))
    }
    pub fn add(&mut self, pos: BlockPos, kind: PoiTypeId) -> Result<bool, PoiError> {
        self.check_y(pos)?;
        let chunk = ChunkPos::containing(pos);
        self.chunk(chunk)?;
        let changed = self
            .chunks
            .get_mut(&chunk)
            .unwrap()
            .sections
            .entry(pos.y >> 4)
            .or_default()
            .add(PoiRecord::new(pos, kind));
        if changed {
            self.dirty(chunk);
        }
        Ok(changed)
    }
    pub fn remove(&mut self, pos: BlockPos) -> Result<bool, PoiError> {
        let chunk = ChunkPos::containing(pos);
        self.chunk(chunk)?;
        let changed = self
            .chunks
            .get_mut(&chunk)
            .unwrap()
            .sections
            .get_mut(&(pos.y >> 4))
            .is_some_and(|section| section.remove(pos));
        if changed {
            self.dirty(chunk);
        }
        Ok(changed)
    }
    pub fn get_type(&self, pos: BlockPos) -> Result<Option<PoiTypeId>, PoiError> {
        Ok(self
            .chunk(ChunkPos::containing(pos))?
            .sections
            .get(&(pos.y >> 4))
            .and_then(|section| section.get(pos))
            .map(|record| record.poi_type))
    }
    pub fn get_free_tickets(&self, pos: BlockPos) -> Result<i32, PoiError> {
        Ok(self
            .chunk(ChunkPos::containing(pos))?
            .sections
            .get(&(pos.y >> 4))
            .and_then(|section| section.get(pos))
            .map_or(0, |record| record.free_tickets))
    }
    pub fn exists(
        &self,
        pos: BlockPos,
        mut predicate: impl FnMut(PoiTypeId) -> bool,
    ) -> Result<bool, PoiError> {
        Ok(self.get_type(pos)?.is_some_and(&mut predicate))
    }
    pub fn release(&mut self, pos: BlockPos) -> Result<bool, PoiError> {
        let chunk = ChunkPos::containing(pos);
        self.chunk(chunk)?;
        let record = self
            .chunks
            .get_mut(&chunk)
            .unwrap()
            .sections
            .get_mut(&(pos.y >> 4))
            .and_then(|section| section.get_mut(pos))
            .ok_or(PoiError::MissingRecord(pos))?;
        let released = record.release_ticket();
        // PoiSection.release marks dirty even if capacity was already full.
        self.dirty(chunk);
        Ok(released)
    }
    pub fn get_in_chunk(
        &self,
        pos: ChunkPos,
        mut predicate: impl FnMut(PoiTypeId) -> bool,
        occupancy: Occupancy,
    ) -> Result<Vec<PoiRecord>, PoiError> {
        let chunk = self.chunk(pos)?;
        Ok(chunk
            .sections
            .range(self.min_section..=self.max_section)
            .flat_map(|(_, section)| section.records(&mut predicate, occupancy))
            .collect())
    }
    /// Port of ChunkPos.rangeClosed: X changes fastest, then Z.
    pub fn chunks_in_square(center: BlockPos, radius: i32) -> Vec<ChunkPos> {
        let center = ChunkPos::containing(center);
        let r = radius.div_euclid(16) + 1;
        let mut chunks = Vec::new();
        for z in center.z - r..=center.z + r {
            for x in center.x - r..=center.x + r {
                chunks.push(ChunkPos::new(x, z));
            }
        }
        chunks
    }
    pub fn get_in_square(
        &self,
        mut predicate: impl FnMut(PoiTypeId) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Result<Vec<PoiRecord>, PoiError> {
        let mut result = Vec::new();
        for chunk in Self::chunks_in_square(center, radius) {
            result.extend(
                self.get_in_chunk(chunk, &mut predicate, occupancy)?
                    .into_iter()
                    .filter(|record| {
                        (record.pos.x.wrapping_sub(center.x)).wrapping_abs() <= radius
                            && (record.pos.z.wrapping_sub(center.z)).wrapping_abs() <= radius
                    }),
            );
        }
        Ok(result)
    }
    pub fn distance_squared(a: BlockPos, b: BlockPos) -> f64 {
        let dx = f64::from(a.x) - f64::from(b.x);
        let dy = f64::from(a.y) - f64::from(b.y);
        let dz = f64::from(a.z) - f64::from(b.z);
        dx * dx + dy * dy + dz * dz
    }
    pub fn get_in_range(
        &self,
        predicate: impl FnMut(PoiTypeId) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Result<Vec<PoiRecord>, PoiError> {
        let square = radius.wrapping_mul(radius);
        Ok(self
            .get_in_square(predicate, center, radius, occupancy)?
            .into_iter()
            .filter(|record| Self::distance_squared(record.pos, center) <= f64::from(square))
            .collect())
    }
    pub fn get_count_in_range(
        &self,
        predicate: impl FnMut(PoiTypeId) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Result<usize, PoiError> {
        Ok(self
            .get_in_range(predicate, center, radius, occupancy)?
            .len())
    }
    pub fn find(
        &self,
        predicate: impl FnMut(PoiTypeId) -> bool,
        mut filter: impl FnMut(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Result<Option<BlockPos>, PoiError> {
        Ok(self
            .get_in_range(predicate, center, radius, occupancy)?
            .into_iter()
            .find(|record| filter(record.pos))
            .map(|record| record.pos))
    }
    pub fn find_closest(
        &self,
        predicate: impl FnMut(PoiTypeId) -> bool,
        mut filter: impl FnMut(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Result<Option<BlockPos>, PoiError> {
        let mut closest = None;
        let mut best = f64::INFINITY;
        for record in self.get_in_range(predicate, center, radius, occupancy)? {
            if filter(record.pos) {
                let distance = Self::distance_squared(record.pos, center);
                if distance < best {
                    best = distance;
                    closest = Some(record.pos);
                }
            }
        }
        Ok(closest)
    }
    pub fn take(
        &mut self,
        predicate: impl FnMut(PoiTypeId) -> bool,
        mut filter: impl FnMut(PoiTypeId, BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
    ) -> Result<Option<BlockPos>, PoiError> {
        for record in self.get_in_range(predicate, center, radius, Occupancy::HasSpace)? {
            if filter(record.poi_type, record.pos) {
                let chunk = ChunkPos::containing(record.pos);
                let record = self
                    .chunks
                    .get_mut(&chunk)
                    .unwrap()
                    .sections
                    .get_mut(&(record.pos.y >> 4))
                    .unwrap()
                    .record_for_type_mut(record.poi_type, record.pos)
                    .expect("queried POI");
                record.acquire_ticket();
                let pos = record.pos;
                self.dirty(chunk);
                return Ok(Some(pos));
            }
        }
        Ok(None)
    }
    pub fn get_random(
        &self,
        predicate: impl FnMut(PoiTypeId) -> bool,
        mut filter: impl FnMut(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
        random: &mut impl RandomSource,
    ) -> Result<Option<BlockPos>, PoiError> {
        let mut records = self.get_in_range(predicate, center, radius, occupancy)?;
        for remaining in (2..=records.len()).rev() {
            let selected = random.next_int_bound(remaining as i32) as usize;
            records.swap(remaining - 1, selected);
        }
        Ok(records
            .into_iter()
            .find(|record| filter(record.pos))
            .map(|record| record.pos))
    }
    pub fn check_consistency(
        &mut self,
        chunk: ChunkPos,
        section_y: i32,
        points: Vec<(BlockPos, PoiTypeId)>,
    ) -> Result<(), PoiError> {
        self.chunk(chunk)?;
        let sections = &mut self.chunks.get_mut(&chunk).unwrap().sections;
        let changed = if let Some(section) = sections.get_mut(&section_y) {
            section.refresh(points)
        } else if points.is_empty() {
            false
        } else {
            let mut section = PoiSection::default();
            for (pos, kind) in points {
                section.add(PoiRecord::new(pos, kind));
            }
            sections.insert(section_y, section);
            true
        };
        if changed {
            self.dirty(chunk);
        }
        Ok(())
    }
    pub fn capture_chunk_for_save(&self, pos: ChunkPos) -> Result<Option<PoiChunkSave>, PoiError> {
        let chunk = self.chunk(pos)?;
        if !chunk.dirty {
            return Ok(None);
        }
        Ok(Some(PoiChunkSave {
            revision: chunk.revision,
            chunk: PoiChunk {
                data_version: Some(ferraloom_storage::DATA_VERSION),
                sections: Some(
                    chunk
                        .sections
                        .iter()
                        .map(|(y, section)| (y.to_string().into(), section.pack()))
                        .collect(),
                ),
                ..Default::default()
            },
        }))
    }
    pub fn acknowledge_chunk_saved(
        &mut self,
        pos: ChunkPos,
        revision: u64,
    ) -> Result<(), PoiError> {
        self.chunk(pos)?;
        let chunk = self.chunks.get_mut(&pos).unwrap();
        if chunk.revision == revision {
            chunk.dirty = false;
        }
        Ok(())
    }
}
