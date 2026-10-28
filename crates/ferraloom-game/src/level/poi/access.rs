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
    fn configure_poi_bounds(&mut self) {
        let min = self.min_build_height() >> 4;
        let max = self.max_build_height() >> 4;
        self.region_mut().poi.set_section_bounds(min, max);
    }
    pub fn poi_manager(&self) -> &PoiManager {
        &self.region().poi
    }
    /// Port of SectionStorage#getOrLoad. No FULL request, ticket or tick advance.
    pub fn ensure_poi_chunk_loaded(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        if self.poi_manager().is_chunk_loaded(pos) {
            return Ok(());
        }
        let data = self
            .acquire_driver()
            .ok_or(LevelError::Poi(PoiError::MissingStorage))?
            .load_poi_chunk(pos)?;
        self.complete_poi_chunk_load(pos, data)
    }
    pub fn complete_poi_chunk_load(
        &mut self,
        pos: ChunkPos,
        data: Option<ferraloom_storage::PoiChunk>,
    ) -> Result<(), LevelError> {
        self.configure_poi_bounds();
        self.region_mut()
            .poi
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
        self.ensure_poi_chunk_loaded(pos)?;
        for (index, section) in sections.iter().enumerate() {
            let y = min_section + index as i32;
            let points = section_points(pos, y, section);
            self.region_mut()
                .poi
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
                .check_consistency(pos, y, points)
                .map_err(LevelError::Poi)?;
        }
        Ok(())
    }
    pub fn capture_poi_chunk_for_save(
        &self,
        pos: ChunkPos,
    ) -> Result<Option<PoiChunkSave>, LevelError> {
        self.poi_manager()
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
            .acknowledge_chunk_saved(pos, revision)
            .map_err(LevelError::Poi)
    }
}
