use super::{PoiChunkSave, PoiError, PoiManager, PoiTypeId, PoiTypes};
use crate::level::{Level, LevelError};
use ferraloom_chunk::LevelChunkSection;
use ferraloom_util::{BlockPos, ChunkPos};

fn section_points(
    chunk: ChunkPos,
    y: i32,
    section: &LevelChunkSection,
) -> Vec<(BlockPos, PoiTypeId)> {
    let types = PoiTypes::builtin();
    if !section.states().maybe_has(|state| types.has_poi(state)) {
        return Vec::new();
    }
    let mut points = Vec::new();
    // SectionPos.blocksInside -> BlockPos.betweenClosed: X fastest, then Y,Z.
    for z in 0..16 {
        for local_y in 0..16 {
            for x in 0..16 {
                if let Some(kind) = types.for_state(section.get_block_state(x, local_y, z)) {
                    points.push((
                        BlockPos::new(
                            chunk.x * 16 + i32::from(x),
                            y * 16 + i32::from(local_y),
                            chunk.z * 16 + i32::from(z),
                        ),
                        kind,
                    ));
                }
            }
        }
    }
    points
}
impl Level {
    /// Exclusive query access cannot retain a shared-state guard across predicates.
    pub fn poi_manager(&mut self) -> &mut PoiManager {
        self.region_mut().poi.get_mut()
    }
    /// Port of SectionStorage#getOrLoad. No FULL request, ticket or tick advance.
    pub fn ensure_poi_chunk_loaded(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        self.ensure_poi_chunk_loaded_shared(pos)
    }
    /// POI-only read. The loaded-state guard ends before driver or storage calls.
    pub fn ensure_poi_chunk_loaded_shared(&self, pos: ChunkPos) -> Result<(), LevelError> {
        if self.region().poi.borrow().is_chunk_loaded(pos) {
            return Ok(());
        }
        let data = self
            .acquire_driver()
            .ok_or(LevelError::Poi(PoiError::MissingStorage))?
            .load_poi_chunk(pos)?;
        self.complete_poi_chunk_load_shared(pos, data)
    }
    pub fn complete_poi_chunk_load(
        &mut self,
        pos: ChunkPos,
        data: Option<ferraloom_storage::PoiChunk>,
    ) -> Result<(), LevelError> {
        self.complete_poi_chunk_load_shared(pos, data)
    }
    /// SectionStorage load completion only: no IO, callbacks or world dispatch.
    /// First completion wins, including a nested synchronous load's known absence.
    pub fn complete_poi_chunk_load_shared(
        &self,
        pos: ChunkPos,
        data: Option<ferraloom_storage::PoiChunk>,
    ) -> Result<(), LevelError> {
        let min = self.min_build_height() >> 4;
        let max = self.max_build_height() >> 4;
        let mut manager = self.region().poi.borrow_mut();
        manager.set_section_bounds(min, max);
        manager
            .complete_chunk_load(pos, data)
            .map_err(LevelError::Poi)
    }
    /// SerializableChunkData#read validates POI for persisted PROTO as well as FULL.
    pub fn check_poi_consistency_with_sections(
        &mut self,
        pos: ChunkPos,
        min_section: i32,
        sections: &[LevelChunkSection],
    ) -> Result<(), LevelError> {
        self.check_poi_consistency_with_sections_shared(pos, min_section, sections)
    }
    /// Scan immutable registry/section data before borrowing the canonical manager.
    pub fn check_poi_consistency_with_sections_shared(
        &self,
        pos: ChunkPos,
        min_section: i32,
        sections: &[LevelChunkSection],
    ) -> Result<(), LevelError> {
        self.ensure_poi_chunk_loaded_shared(pos)?;
        for (index, section) in sections.iter().enumerate() {
            let y = min_section + index as i32;
            let points = section_points(pos, y, section);
            self.region()
                .poi
                .borrow_mut()
                .check_consistency(pos, y, points)
                .map_err(LevelError::Poi)?;
        }
        Ok(())
    }
    pub fn check_poi_consistency_with_blocks(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        self.ensure_poi_chunk_loaded(pos)?;
        let chunk = self.chunk(pos).ok_or(LevelError::MissingChunk(pos))?;
        let points: Vec<_> = chunk
            .sections()
            .enumerate()
            .map(|(index, section)| {
                let y = chunk.min_section() + index as i32;
                (y, section_points(pos, y, section))
            })
            .collect();
        for (y, points) in points {
            self.region_mut()
                .poi
                .get_mut()
                .check_consistency(pos, y, points)
                .map_err(LevelError::Poi)?;
        }
        Ok(())
    }
    pub fn capture_poi_chunk_for_save(
        &self,
        pos: ChunkPos,
    ) -> Result<Option<PoiChunkSave>, LevelError> {
        self.region()
            .poi
            .borrow()
            .capture_chunk_for_save(pos)
            .map_err(LevelError::Poi)
    }
    pub fn acknowledge_poi_chunk_saved(
        &mut self,
        pos: ChunkPos,
        revision: u64,
    ) -> Result<(), LevelError> {
        self.region_mut()
            .poi
            .get_mut()
            .acknowledge_chunk_saved(pos, revision)
            .map_err(LevelError::Poi)
    }
}
