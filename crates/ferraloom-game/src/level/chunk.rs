use super::{distance_manager::FullChunkStatus, heightmap::Heightmap};
use crate::ticks::LevelChunkTicks;
use ferraloom_worldgen::structure::pipeline::ChunkStructures;
use std::{
    cell::{Cell, OnceCell},
    ops::Range,
};

pub use ferraloom_chunk::LevelChunkSection;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos};

/// Port of LevelChunk's section ownership. Absent sections use the supplied
/// empty-section template, including the dimension's default biome registry.
pub struct LevelChunk {
    pub(super) publication_generation: u64,
    pub(super) staged_publication: Option<super::publication::StagedPublication>,
    pub(super) loaded: Cell<bool>,
    pos: ChunkPos,
    min_section: i32,
    sections: Vec<Option<LevelChunkSection>>,
    empty_section: LevelChunkSection,
    heightmaps: Option<[[i32; 256]; 4]>,
    generation_heightmaps: [OnceCell<[i32; 256]>; 2],
    pub(super) block_ticks: LevelChunkTicks,
    pub(super) fluid_ticks: LevelChunkTicks<ferraloom_data::registries::fluid::Id>,
    requested_full_status: Cell<FullChunkStatus>,
    ticking_ready: Cell<bool>,
    pub(super) block_entities: super::block_entity::ChunkBlockEntities,
    inhabited_time: i64,
    pub(super) pending_generation: super::post_processing::PendingGenerationData,
    pub(super) structures: super::structure_manager::StructureVersions,
    unsaved: Cell<bool>,
}

impl LevelChunk {
    /// Port of ChunkAccess#markUnsaved and tryMarkSaved. Clear immediately
    /// before capturing a save; later mutations set the flag again independently.
    pub fn mark_unsaved(&self) {
        self.unsaved.set(true);
    }
    pub fn try_mark_saved(&mut self) -> bool {
        self.collect_structure_versions();
        self.unsaved.replace(false)
    }
    pub fn is_unsaved(&self) -> bool {
        self.unsaved.get()
    }
    pub fn new(pos: ChunkPos, sections: Range<i32>, empty_section: LevelChunkSection) -> Self {
        assert!(empty_section.has_only_air());
        Self {
            publication_generation: 0,
            staged_publication: None,
            loaded: Cell::new(false),
            pos,
            min_section: sections.start,
            sections: sections.map(|_| None).collect(),
            empty_section,
            heightmaps: None,
            generation_heightmaps: std::array::from_fn(|_| OnceCell::new()),
            block_ticks: LevelChunkTicks::default(),
            fluid_ticks: LevelChunkTicks::default(),
            // LevelChunk#getFullStatus defaults to FULL until a holder's
            // supplier is attached. This alone never permits broadcasts.
            requested_full_status: Cell::new(FullChunkStatus::Full),
            ticking_ready: Cell::new(false),
            block_entities: Default::default(),
            inhabited_time: 0,
            pending_generation: Default::default(),
            structures: super::structure_manager::StructureVersions::new(pos),
            unsaved: Cell::new(false),
        }
    }
    pub fn from_sections(
        pos: ChunkPos,
        min_section: i32,
        sections: Vec<LevelChunkSection>,
    ) -> Self {
        assert!(!sections.is_empty());
        let first = &sections[0];
        let empty_section = LevelChunkSection::new(
            first.states().strategy().clone(),
            first.biomes().strategy().clone(),
            first.biomes().get(0, 0, 0),
        );
        Self {
            pos,
            min_section,
            publication_generation: 0,
            staged_publication: None,
            loaded: Cell::new(false),
            sections: sections.into_iter().map(Some).collect(),
            empty_section,
            heightmaps: None,
            generation_heightmaps: std::array::from_fn(|_| OnceCell::new()),
            block_ticks: LevelChunkTicks::default(),
            fluid_ticks: LevelChunkTicks::default(),
            requested_full_status: Cell::new(FullChunkStatus::Full),
            ticking_ready: Cell::new(false),
            block_entities: Default::default(),
            inhabited_time: 0,
            pending_generation: Default::default(),
            structures: super::structure_manager::StructureVersions::new(pos),
            unsaved: Cell::new(false),
        }
    }
    pub fn min_y(&self) -> i32 {
        self.min_section * 16
    }
    /// Port of LevelChunk#isLoaded; remains false throughout entity postLoad.
    pub fn is_loaded(&self) -> bool {
        self.loaded.get()
    }
    pub(crate) fn full_publication_complete(&self) -> bool {
        self.loaded.get()
            && self.staged_publication.as_ref().is_none_or(|stage| {
                stage.step.get() == super::publication::FullChunkFinalizationStep::Finalized
            })
    }
    /// Port of ChunkAccess#getInhabitedTime, saved as InhabitedTime.
    pub fn inhabited_time(&self) -> i64 {
        self.inhabited_time
    }
    pub fn set_inhabited_time(&mut self, inhabited_time: i64) {
        self.inhabited_time = inhabited_time;
    }
    /// ServerChunkCache#tickSpawningChunk increments this before thunder and
    /// spawning. Native incrementInhabitedTime does not itself mark unsaved.
    pub fn increment_inhabited_time(&mut self) {
        self.inhabited_time = self.inhabited_time.wrapping_add(1);
    }
    pub fn max_y(&self) -> i32 {
        self.min_y() + self.sections.len() as i32 * 16
    }
    pub fn section(&self, section_y: i32) -> Option<&LevelChunkSection> {
        let index = usize::try_from(section_y - self.min_section).ok()?;
        self.sections
            .get(index)
            .map(|s| s.as_ref().unwrap_or(&self.empty_section))
    }
    /// Port of LevelChunk#getFullStatus's holder-backed supplier. Requested
    /// status is observable before the corresponding ticking future completes.
    pub fn full_status(&self) -> FullChunkStatus {
        self.requested_full_status.get()
    }
    pub fn ticking_ready(&self) -> bool {
        self.ticking_ready.get()
    }
    pub(super) fn set_loading_status(&self, requested: FullChunkStatus, ticking_ready: bool) {
        self.requested_full_status.set(requested);
        self.ticking_ready.set(ticking_ready);
    }
    pub fn heightmap(&self, map: Heightmap) -> Option<&[i32; 256]> {
        self.heightmaps.as_ref().map(|maps| &maps[map as usize])
    }
    pub fn height_at(&self, map: Heightmap, x: u8, z: u8) -> Option<i32> {
        self.heightmaps
            .as_ref()
            .map(|h| h[map as usize][usize::from(x) + usize::from(z) * 16])
    }
    /// Port of ChunkAccess#getHeight's lazy WG map priming on a live chunk.
    /// LevelChunk#setBlockState updates only its four live maps; these two
    /// generation maps retain their first prime, as the native EnumMap does.
    pub(super) fn generation_height_at(
        &self,
        ocean_floor: bool,
        x: u8,
        z: u8,
        tags: &ferraloom_data::datapack::TagSet<Block>,
    ) -> i32 {
        let map = if ocean_floor {
            Heightmap::OceanFloor
        } else {
            Heightmap::WorldSurface
        };
        let heights = self.generation_heightmaps[usize::from(ocean_floor)].get_or_init(|| {
            std::array::from_fn(|i| {
                self.scan_height(
                    map,
                    BlockPos::new(
                        self.pos.x * 16 + (i & 15) as i32,
                        self.max_y() - 1,
                        self.pos.z * 16 + (i >> 4) as i32,
                    ),
                    tags,
                )
            })
        });
        heights[usize::from(x) + usize::from(z) * 16]
    }
    pub fn prime_heightmaps(&mut self, tags: &ferraloom_data::datapack::TagSet<Block>) {
        if self.heightmaps.is_some() {
            return;
        }
        self.heightmaps = Some(std::array::from_fn(|m| {
            std::array::from_fn(|i| {
                let pos = BlockPos::new(
                    self.pos.x * 16 + (i & 15) as i32,
                    self.max_y() - 1,
                    self.pos.z * 16 + (i >> 4) as i32,
                );
                self.scan_height(Heightmap::ALL[m], pos, tags)
            })
        }));
    }
    fn scan_height(
        &self,
        map: Heightmap,
        mut pos: BlockPos,
        tags: &ferraloom_data::datapack::TagSet<Block>,
    ) -> i32 {
        while pos.y >= self.min_y() {
            if map.matches(self.get_block_state(pos), tags) {
                return pos.y + 1;
            }
            pos.y -= 1;
        }
        self.min_y()
    }
    pub(super) fn update_heightmaps(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        tags: &ferraloom_data::datapack::TagSet<Block>,
    ) {
        let column = ((pos.x & 15) + (pos.z & 15) * 16) as usize;
        for map in Heightmap::ALL {
            let first =
                self.heightmaps.as_ref().expect("primed live heightmaps")[map as usize][column];
            if pos.y <= first - 2 {
                continue;
            }
            let next = if map.matches(state, tags) {
                if pos.y >= first {
                    Some(pos.y + 1)
                } else {
                    None
                }
            } else if first - 1 == pos.y {
                Some(self.scan_height(map, pos.below(1), tags))
            } else {
                None
            };
            if let Some(next) = next {
                self.heightmaps.as_mut().unwrap()[map as usize][column] = next;
            }
        }
    }
    pub fn sections(&self) -> impl Iterator<Item = &LevelChunkSection> {
        self.sections
            .iter()
            .map(|s| s.as_ref().unwrap_or(&self.empty_section))
    }
    pub fn min_section(&self) -> i32 {
        self.min_section
    }
    /// Port of LevelChunk's proto constructor setAllStarts/setAllReferences.
    /// Install the already decoded payload before attachment; no second decoder.
    pub fn set_structures(
        &mut self,
        payload: ChunkStructures,
        revision: u64,
    ) -> Result<(), super::LevelError> {
        self.structures.replace(self.pos, payload, revision)?;
        self.mark_unsaved();
        Ok(())
    }
    pub fn structures(&self) -> &ChunkStructures {
        &self.structures.current().payload
    }
    pub fn structures_revision(&self) -> u64 {
        self.structures.current().revision
    }
    pub(super) fn publish_structures(
        &self,
        payload: ChunkStructures,
        revision: u64,
    ) -> Result<bool, super::LevelError> {
        let published = self.structures.publish(self.pos, payload, revision)?;
        if published {
            self.mark_unsaved();
        }
        Ok(published)
    }
    /// Exclusive phase boundary: previously returned structure references are no
    /// longer live. Saves also collect these old immutable versions.
    pub fn collect_structure_versions(&mut self) {
        self.structures.collect();
    }
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }
    fn section_index(&self, pos: BlockPos) -> Option<usize> {
        if ChunkPos::containing(pos) != self.pos {
            return None;
        }
        usize::try_from((pos.y >> 4) - self.min_section)
            .ok()
            .filter(|&index| index < self.sections.len())
    }
    #[inline]
    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        let Some(index) = self.section_index(pos) else {
            return Block::AIR.default_state();
        };
        self.sections[index]
            .as_ref()
            .unwrap_or(&self.empty_section)
            .get_block_state((pos.x & 15) as u8, (pos.y & 15) as u8, (pos.z & 15) as u8)
    }
    #[inline]
    pub fn set_block_state(&mut self, pos: BlockPos, state: BlockState) -> Option<BlockState> {
        let index = self.section_index(pos)?;
        let section = self.sections[index].get_or_insert_with(|| self.empty_section.clone());
        Some(section.set_block_state(
            (pos.x & 15) as u8,
            (pos.y & 15) as u8,
            (pos.z & 15) as u8,
            state,
        ))
    }
}

impl super::Level {
    /// ChunkMap#save's tryMarkSaved step, before snapshot capture, never at IO
    /// completion. The caller must recheck is_unsaved before final eviction.
    pub fn try_mark_chunk_saved(&mut self, pos: ChunkPos) -> Result<bool, super::LevelError> {
        self.region_mut()
            .chunks
            .get_mut(&pos)
            .map(LevelChunk::try_mark_saved)
            .ok_or(super::LevelError::MissingChunk(pos))
    }
    pub fn mark_chunk_unsaved(&mut self, pos: ChunkPos) -> Result<(), super::LevelError> {
        self.region_mut()
            .chunks
            .get_mut(&pos)
            .ok_or(super::LevelError::MissingChunk(pos))?
            .mark_unsaved();
        Ok(())
    }
}
