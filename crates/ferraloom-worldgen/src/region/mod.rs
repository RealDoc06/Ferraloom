//! Port of WorldGenRegion: exclusive job-local chunks, bounded writes and
//! status-checked reads. The scheduler owns the status DAG and lends disjoint
//! neighborhoods; generation never reaches into a live level.
use crate::{Error, generator::GeneratedChunk};
use ferraloom_chunk::BiomeId;
use ferraloom_data::{Block, BlockState, FluidState};
use ferraloom_nbt::{CompoundTag, Tag};
use std::{cell::OnceCell, collections::BTreeMap};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ChunkStatus {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Terrain,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Heightmap {
    WorldSurfaceWg,
    WorldSurface,
    OceanFloorWg,
    OceanFloor,
    MotionBlocking,
    MotionBlockingNoLeaves,
}
impl Heightmap {
    pub const ALL: [Self; 6] = [
        Self::WorldSurfaceWg,
        Self::WorldSurface,
        Self::OceanFloorWg,
        Self::OceanFloor,
        Self::MotionBlocking,
        Self::MotionBlockingNoLeaves,
    ];
    fn after(self, status: ChunkStatus) -> bool {
        if status < ChunkStatus::Terrain {
            matches!(self, Self::WorldSurfaceWg | Self::OceanFloorWg)
        } else {
            !matches!(self, Self::WorldSurfaceWg | Self::OceanFloorWg)
        }
    }
}

/// Port of SavedTick in ProtoChunkTicks. Worldgen discards trigger times and
/// preserves the first position/type tick and its priority in insertion order.
#[derive(Clone, Debug)]
pub struct SavedTick<T> {
    pub position: [i32; 3],
    pub kind: T,
    pub priority: i32,
}
#[derive(Clone, Debug)]
pub struct ProtoChunkTicks<T> {
    ticks: Vec<SavedTick<T>>,
}
impl<T> Default for ProtoChunkTicks<T> {
    fn default() -> Self {
        Self { ticks: Vec::new() }
    }
}
impl<T: PartialEq> ProtoChunkTicks<T> {
    pub fn schedule(&mut self, tick: SavedTick<T>) {
        if !self.has_scheduled_tick(tick.position, &tick.kind) {
            self.ticks.push(tick);
        }
    }
    pub fn has_scheduled_tick(&self, position: [i32; 3], kind: &T) -> bool {
        self.ticks
            .iter()
            .any(|t| t.position == position && &t.kind == kind)
    }
    pub fn ticks(&self) -> &[SavedTick<T>] {
        &self.ticks
    }
}

/// Owned inputs and outputs of an off-tick worldgen job. Block entities remain
/// NBT until the game layer converts the protochunk to a LevelChunk.
#[derive(Clone, Debug)]
pub struct RegionChunk {
    pub chunk: GeneratedChunk,
    pub status: ChunkStatus,
    pub block_entities: BTreeMap<[i32; 3], CompoundTag>,
    pub entities: Vec<CompoundTag>,
    pub block_ticks: ProtoChunkTicks<Block>,
    pub fluid_ticks: ProtoChunkTicks<ferraloom_data::registries::fluid::Id>,
    pub poi_changes: Vec<([i32; 3], BlockState, BlockState)>,
    heightmaps: [OnceCell<[i32; 256]>; 6],
}
impl RegionChunk {
    pub fn new(chunk: GeneratedChunk, status: ChunkStatus) -> Self {
        let heightmaps: [OnceCell<[i32; 256]>; 6] = std::array::from_fn(|_| OnceCell::new());
        heightmaps[Heightmap::WorldSurfaceWg as usize]
            .set(chunk.world_surface)
            .unwrap();
        heightmaps[Heightmap::OceanFloorWg as usize]
            .set(chunk.ocean_floor)
            .unwrap();
        Self {
            chunk,
            status,
            block_entities: BTreeMap::new(),
            entities: Vec::new(),
            block_ticks: ProtoChunkTicks::default(),
            fluid_ticks: ProtoChunkTicks::default(),
            poi_changes: Vec::new(),
            heightmaps,
        }
    }
}

/// Context-dependent block behavior is supplied by the game adapter. Baseline
/// extracted collision shapes cannot answer survival or neighbor shape queries.
pub trait WorldgenBlockBehavior: Sync {
    // --- ore and cave family ---
    fn multiface_state_for_placement(
        &self,
        _region: &WorldGenRegion<'_>,
        _block: Block,
        _old: BlockState,
        _p: [i32; 3],
        _face: ferraloom_util::Direction,
    ) -> Result<Option<BlockState>, Error> {
        Err(Error("Unsupported: multiface_state_for_placement".into()))
    }
    #[allow(clippy::too_many_arguments)]
    fn multiface_spread(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _state: BlockState,
        _p: [i32; 3],
        _face: ferraloom_util::Direction,
        _random: &mut ferraloom_util::random::WorldgenRandom,
        _mark_post_processing: bool,
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: multiface_spread".into()))
    }
    fn sculk_can_spread_from(
        &self,
        _region: &WorldGenRegion<'_>,
        _p: [i32; 3],
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: sculk_can_spread_from".into()))
    }
    #[allow(clippy::too_many_arguments)]
    fn sculk_spread(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _origin: [i32; 3],
        _random: &mut ferraloom_util::random::WorldgenRandom,
        _charge_count: i32,
        _amount_per_charge: i32,
        _spread_attempts: i32,
        _growth_rounds: i32,
        _spread_rounds: i32,
    ) -> Result<(), Error> {
        Err(Error("Unsupported: sculk_spread".into()))
    }
    fn set_loot_table(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _table: &crate::Identifier,
        _random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        Err(Error(
            "Unsupported: RandomizableContainer#setBlockEntityLootTable".into(),
        ))
    }
    fn set_spawner_entity(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _kind: ferraloom_data::EntityType,
        _random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        Err(Error("Unsupported: SpawnerBlockEntity#setEntityId".into()))
    }
    // --- end ore and cave family ---
    fn destroy_block(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _drop: bool,
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: destroy_block".into()))
    }
    // --- tree family ---
    fn populate_worldgen_beehive(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        Err(Error("Unsupported: populate_worldgen_beehive".into()))
    }
    fn state_has_sturdy_face(
        &self,
        _region: &WorldGenRegion<'_>,
        _state: BlockState,
        _query_position: [i32; 3],
        _face: ferraloom_util::Direction,
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: state_has_sturdy_face".into()))
    }
    // --- end tree family ---

    fn place_liquid(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _fluid: FluidState,
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: place_liquid".into()))
    }
    fn update_shape(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _direction: ferraloom_util::Direction,
        _neighbor_p: [i32; 3],
        _neighbor_state: BlockState,
    ) -> Result<BlockState, Error> {
        Err(Error("Unsupported: update_shape".into()))
    }
    fn update_from_neighbor_shapes(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _state: BlockState,
    ) -> Result<BlockState, Error> {
        Err(Error("Unsupported: update_from_neighbor_shapes".into()))
    }
    fn update_neighbors_at(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _block: Block,
    ) -> Result<(), Error> {
        Err(Error("Unsupported: update_neighbors_at".into()))
    }
    fn load_block_entity(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _tag: CompoundTag,
        _random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        Err(Error("Unsupported: load_block_entity".into()))
    }
    fn finalize_template_entity(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _tag: &mut CompoundTag,
        _mirror: Mirror,
        _rotation: Rotation,
        _finalize: bool,
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: finalize_template_entity".into()))
    }

    // --- vegetation family ---
    fn vine_acceptable_neighbor(
        &self,
        _region: &WorldGenRegion<'_>,
        _p: [i32; 3],
        _direction: ferraloom_util::Direction,
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: VineBlock.isAcceptableNeighbour".into()))
    }
    fn place_double_plant(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _state: BlockState,
        _p: [i32; 3],
        _flags: u32,
    ) -> Result<(), Error> {
        Err(Error("Unsupported: DoublePlantBlock.placeAt".into()))
    }
    // MossyCarpetBlock.placeAt uses Level#getRandom, not the feature stream.
    fn place_mossy_carpet(
        &self,
        _region: &mut WorldGenRegion<'_>,
        _p: [i32; 3],
        _flags: u32,
    ) -> Result<(), Error> {
        Err(Error(
            "Unsupported: MossyCarpetBlock.placeAt and level random".into(),
        ))
    }
    // --- end vegetation family ---

    fn has_block_entity(&self, state: BlockState) -> bool {
        state.has_block_entity()
    }
    fn post_process_pos(
        &self,
        state: BlockState,
        region: &WorldGenRegion<'_>,
        position: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error>;
    fn is_outline_full_block(
        &self,
        _region: &WorldGenRegion<'_>,
        _state: BlockState,
        _p: [i32; 3],
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: is_outline_full_block".into()))
    }
    fn is_collision_full_block(
        &self,
        _region: &WorldGenRegion<'_>,
        _state: BlockState,
        _p: [i32; 3],
    ) -> Result<bool, Error> {
        Err(Error("Unsupported: is_collision_full_block".into()))
    }
    fn is_solid(&self, _state: BlockState) -> Result<bool, Error> {
        Err(Error("solid block behavior is unavailable".into()))
    }
    fn has_sturdy_face(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _position: [i32; 3],
        _face: ferraloom_util::Direction,
    ) -> Result<bool, Error> {
        Err(Error("sturdy face behavior is unavailable".into()))
    }
    fn can_survive(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _position: [i32; 3],
    ) -> Result<bool, Error> {
        Err(Error("block survival behavior is unavailable".into()))
    }
    fn is_unobstructed(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _position: [i32; 3],
    ) -> Result<bool, Error> {
        Err(Error("block collision behavior is unavailable".into()))
    }
}

pub use ferraloom_util::core::{Mirror, Rotation};

pub struct WorldGenRegion<'a> {
    block_tags: Option<std::sync::Arc<ferraloom_data::datapack::TagSet<Block>>>,
    chunks: &'a mut [RegionChunk],
    center: [i32; 2],
    dependencies: &'a [ChunkStatus],
    write_radius: i32,
    generation_bounds: Option<(i32, i32)>,
    behavior: &'a dyn WorldgenBlockBehavior,
    // --- vegetation family ---
    has_skylight: Option<bool>,
    // --- end vegetation family ---
    pub seed: i64,
    pub sea_level: i32,
}
impl<'a> WorldGenRegion<'a> {
    // --- tree family ---
    pub fn populate_worldgen_beehive(
        &mut self,
        p: [i32; 3],
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        self.behavior.populate_worldgen_beehive(self, p, random)
    }
    pub fn state_has_sturdy_face(
        &self,
        state: BlockState,
        query_position: [i32; 3],
        face: ferraloom_util::Direction,
    ) -> Result<bool, Error> {
        match state.cached_sturdy_face(face) {
            Some(sturdy) => Ok(sturdy),
            None => self
                .behavior
                .state_has_sturdy_face(self, state, query_position, face),
        }
    }
    // --- end tree family ---

    /// `dependencies[d]` is the maximum requested status at chessboard distance
    /// d, exactly ChunkStep.directDependencies. An upgrading center additionally
    /// restricts writes to its generation height accessor (min Y, height).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chunks: &'a mut [RegionChunk],
        center: [i32; 2],
        dependencies: &'a [ChunkStatus],
        write_radius: i32,
        generation_bounds: Option<(i32, i32)>,
        behavior: &'a dyn WorldgenBlockBehavior,
        seed: i64,
        sea_level: i32,
    ) -> Result<Self, Error> {
        if !chunks.iter().any(|c| c.chunk.position == center) {
            return Err(Error("worldgen center chunk unavailable".into()));
        }
        let mut positions = std::collections::BTreeSet::new();
        if chunks.iter().any(|c| !positions.insert(c.chunk.position)) {
            return Err(Error("duplicate worldgen chunk".into()));
        }
        Ok(Self {
            block_tags: None,
            chunks,
            center,
            dependencies,
            write_radius,
            generation_bounds,
            behavior,
            has_skylight: None,
            seed,
            sea_level,
        })
    }
    // --- ore and cave family ---
    pub fn multiface_state_for_placement(
        &self,
        block: Block,
        old: BlockState,
        p: [i32; 3],
        face: ferraloom_util::Direction,
    ) -> Result<Option<BlockState>, Error> {
        self.behavior
            .multiface_state_for_placement(self, block, old, p, face)
    }
    pub fn multiface_spread(
        &mut self,
        state: BlockState,
        p: [i32; 3],
        face: ferraloom_util::Direction,
        random: &mut ferraloom_util::random::WorldgenRandom,
        mark_post_processing: bool,
    ) -> Result<bool, Error> {
        self.behavior
            .multiface_spread(self, state, p, face, random, mark_post_processing)
    }
    pub fn sculk_can_spread_from(&self, p: [i32; 3]) -> Result<bool, Error> {
        self.behavior.sculk_can_spread_from(self, p)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn sculk_spread(
        &mut self,
        origin: [i32; 3],
        random: &mut ferraloom_util::random::WorldgenRandom,
        charge_count: i32,
        amount_per_charge: i32,
        spread_attempts: i32,
        growth_rounds: i32,
        spread_rounds: i32,
    ) -> Result<(), Error> {
        self.behavior.sculk_spread(
            self,
            origin,
            random,
            charge_count,
            amount_per_charge,
            spread_attempts,
            growth_rounds,
            spread_rounds,
        )
    }
    pub fn set_loot_table(
        &mut self,
        p: [i32; 3],
        table: &crate::Identifier,
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        self.behavior.set_loot_table(self, p, table, random)
    }
    pub fn set_spawner_entity(
        &mut self,
        p: [i32; 3],
        kind: ferraloom_data::EntityType,
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        self.behavior.set_spawner_entity(self, p, kind, random)
    }
    // --- end ore and cave family ---
    pub fn center(&self) -> [i32; 2] {
        self.center
    }
    pub fn set_block_tags(
        &mut self,
        tags: std::sync::Arc<ferraloom_data::datapack::TagSet<Block>>,
    ) {
        self.block_tags = Some(tags);
    }
    pub fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Block>, Error> {
        self.block_tags
            .as_deref()
            .ok_or_else(|| Error("Unsupported: block tag context unavailable".into()))
    }
    fn distance(&self, p: [i32; 2]) -> usize {
        p[0].abs_diff(self.center[0])
            .max(p[1].abs_diff(self.center[1])) as usize
    }
    pub fn has_chunk(&self, p: [i32; 2]) -> bool {
        self.distance(p) < self.dependencies.len()
    }
    fn index(&self, p: [i32; 2], requested: ChunkStatus) -> Result<usize, Error> {
        let max = self.dependencies.get(self.distance(p));
        if max.is_none_or(|&s| requested > s) {
            return Err(Error(format!(
                "chunk {p:?} unavailable at requested status {requested:?}"
            )));
        }
        self.chunks
            .iter()
            .position(|c| c.chunk.position == p)
            .ok_or_else(|| Error(format!("chunk {p:?} missing from worldgen cache")))
    }
    pub fn chunk(&self, p: [i32; 2], requested: ChunkStatus) -> Result<&RegionChunk, Error> {
        Ok(&self.chunks[self.index(p, requested)?])
    }
    pub fn min_y(&self) -> i32 {
        self.chunks
            .iter()
            .find(|c| c.chunk.position == self.center)
            .unwrap()
            .chunk
            .min_y
    }
    pub fn height(&self) -> i32 {
        self.chunks
            .iter()
            .find(|c| c.chunk.position == self.center)
            .unwrap()
            .chunk
            .height
    }
    pub fn block(&self, p: [i32; 3]) -> Result<BlockState, Error> {
        Ok(self
            .chunk([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?
            .chunk
            .block((p[0] & 15) as u8, p[1], (p[2] & 15) as u8))
    }
    pub fn fluid(&self, p: [i32; 3]) -> Result<FluidState, Error> {
        Ok(self.block(p)?.data().fluid)
    }
    pub fn noise_biome(&self, quart: [i32; 3]) -> Result<BiomeId, Error> {
        Ok(self
            .chunk([quart[0] >> 2, quart[2] >> 2], ChunkStatus::Empty)?
            .chunk
            .biome(quart))
    }
    pub fn height_at(&self, map: Heightmap, x: i32, z: i32) -> Result<i32, Error> {
        let region_chunk = self.chunk([x >> 4, z >> 4], ChunkStatus::Empty)?;
        let heights = region_chunk.heightmaps[map as usize].get_or_init(|| {
            std::array::from_fn(|i| {
                region_chunk
                    .chunk
                    .scan_height(map, (i & 15) as u8, (i >> 4) as u8)
            })
        });
        Ok(heights[((z & 15) * 16 + (x & 15)) as usize])
    }
    pub fn ensure_can_write(&self, p: [i32; 3]) -> bool {
        self.write_radius >= 0
            && self.distance([p[0] >> 4, p[2] >> 4]) <= self.write_radius as usize
            && self
                .generation_bounds
                .is_none_or(|(min, height)| p[1] >= min && p[1] < min + height)
    }
    pub fn set_block(&mut self, p: [i32; 3], state: BlockState, flags: u32) -> Result<bool, Error> {
        if !self.ensure_can_write(p) {
            return Ok(false);
        }
        let index = self.index([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?;
        if self.chunks[index].status >= ChunkStatus::InitializeLight {
            return Err(Error(
                "worldgen lit chunk mutation needs a game adapter".into(),
            ));
        }
        let old = self.block(p)?;
        let in_height = p[1] >= self.chunks[index].chunk.min_y
            && p[1] < self.chunks[index].chunk.min_y + self.chunks[index].chunk.height;
        let section_y = in_height.then(|| ((p[1] - self.chunks[index].chunk.min_y) >> 4) as usize);
        let skip_heightmaps = section_y.is_some_and(|section| {
            self.chunks[index].chunk.sections[section].has_only_air() && state.block() == Block::AIR
        });
        if self.chunks[index].status >= ChunkStatus::Terrain {
            if in_height {
                self.set_block_in_section(p, state)?;
            }
        } else {
            self.chunks[index]
                .chunk
                .set_block((p[0] & 15) as u8, p[1], (p[2] & 15) as u8, state);
        }
        if in_height && !skip_heightmaps {
            let region_chunk = &mut self.chunks[index];
            let column_index = ((p[2] & 15) * 16 + (p[0] & 15)) as usize;
            for map in Heightmap::ALL
                .into_iter()
                .filter(|map| map.after(region_chunk.status))
            {
                let cache = &mut region_chunk.heightmaps[map as usize];
                if cache.get().is_none() {
                    cache
                        .set(std::array::from_fn(|i| {
                            region_chunk
                                .chunk
                                .scan_height(map, (i & 15) as u8, (i >> 4) as u8)
                        }))
                        .unwrap();
                } else {
                    let heights = cache.get_mut().unwrap();
                    if region_chunk.chunk.heightmap_matches(map, state)
                        && p[1] >= heights[column_index]
                    {
                        heights[column_index] = p[1] + 1;
                    } else if p[1] == heights[column_index] - 1
                        && !region_chunk.chunk.heightmap_matches(map, state)
                    {
                        heights[column_index] = region_chunk.chunk.scan_height_below(
                            map,
                            (p[0] & 15) as u8,
                            (p[2] & 15) as u8,
                            p[1],
                        );
                    }
                }
            }
        }
        self.chunks[index].poi_changes.push((p, old, state));
        if state.has_block_entity() {
            let mut tag = CompoundTag::new();
            for (key, value) in [("x", p[0]), ("y", p[1]), ("z", p[2])] {
                tag.insert(key, Tag::Int(value));
            }
            tag.insert("id", Tag::String("DUMMY".into()));
            self.chunks[index].block_entities.insert(p, tag);
        } else if in_height && old.has_block_entity() {
            self.chunks[index].block_entities.remove(&p);
        }
        if flags & 16 == 0 {
            let post = state
                .post_process_y_offset()
                .map(|offset| [p[0], p[1].wrapping_add(offset), p[2]]);
            if let Some(post) = post {
                self.mark_post_processing(post)?;
            }
        }
        Ok(true)
    }
    /// Port of BulkSectionAccess#getSection followed by setBlockState. Ore
    /// features check write bounds and build height before taking this path.
    /// It intentionally changes only section storage and its derived counts.
    pub fn set_block_in_section(&mut self, p: [i32; 3], state: BlockState) -> Result<(), Error> {
        let index = self.index([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?;
        let chunk = &mut self.chunks[index].chunk;
        if p[1] < chunk.min_y || p[1] >= chunk.min_y + chunk.height {
            return Err(Error("bulk section write outside build height".into()));
        }
        chunk.sections[((p[1] - chunk.min_y) >> 4) as usize].set_block_state(
            (p[0] & 15) as u8,
            (p[1] & 15) as u8,
            (p[2] & 15) as u8,
            state,
        );
        Ok(())
    }
    pub fn mark_post_processing(&mut self, p: [i32; 3]) -> Result<(), Error> {
        let index = self.index([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?;
        let chunk = &mut self.chunks[index].chunk;
        if p[1] >= chunk.min_y && p[1] < chunk.min_y + chunk.height {
            chunk.post_processing.push(p);
        }
        Ok(())
    }
    pub fn set_block_entity(&mut self, p: [i32; 3], tag: CompoundTag) -> Result<(), Error> {
        let index = self.index([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?;
        self.chunks[index].block_entities.insert(p, tag);
        Ok(())
    }
    pub fn block_entity(&self, p: [i32; 3]) -> Result<Option<&CompoundTag>, Error> {
        Ok(self
            .chunk([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?
            .block_entities
            .get(&p))
    }
    pub fn add_entity(&mut self, chunk: [i32; 2], tag: CompoundTag) -> Result<(), Error> {
        let index = self.index(chunk, ChunkStatus::Empty)?;
        self.chunks[index].entities.push(tag);
        Ok(())
    }
    pub fn schedule_block_tick(&mut self, tick: SavedTick<Block>) -> Result<(), Error> {
        let index = self.index(
            [tick.position[0] >> 4, tick.position[2] >> 4],
            ChunkStatus::Empty,
        )?;
        self.chunks[index].block_ticks.schedule(tick);
        Ok(())
    }
    pub fn schedule_fluid_tick(
        &mut self,
        tick: SavedTick<ferraloom_data::registries::fluid::Id>,
    ) -> Result<(), Error> {
        let index = self.index(
            [tick.position[0] >> 4, tick.position[2] >> 4],
            ChunkStatus::Empty,
        )?;
        self.chunks[index].fluid_ticks.schedule(tick);
        Ok(())
    }
    pub fn destroy_block(&mut self, p: [i32; 3], drop: bool) -> Result<bool, Error> {
        self.behavior.destroy_block(self, p, drop)
    }
    pub fn place_liquid(&mut self, p: [i32; 3], fluid: FluidState) -> Result<bool, Error> {
        self.behavior.place_liquid(self, p, fluid)
    }
    pub fn update_shape(
        &mut self,
        p: [i32; 3],
        direction: ferraloom_util::Direction,
        neighbor_p: [i32; 3],
        neighbor_state: BlockState,
    ) -> Result<BlockState, Error> {
        self.behavior
            .update_shape(self, p, direction, neighbor_p, neighbor_state)
    }
    pub fn update_from_neighbor_shapes(
        &mut self,
        p: [i32; 3],
        state: BlockState,
    ) -> Result<BlockState, Error> {
        self.behavior.update_from_neighbor_shapes(self, p, state)
    }
    pub fn update_neighbors_at(&mut self, p: [i32; 3], block: Block) -> Result<(), Error> {
        self.behavior.update_neighbors_at(self, p, block)
    }
    pub fn load_block_entity(
        &mut self,
        p: [i32; 3],
        tag: CompoundTag,
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        self.behavior.load_block_entity(self, p, tag, random)
    }
    pub fn finalize_template_entity(
        &mut self,
        tag: &mut CompoundTag,
        mirror: Mirror,
        rotation: Rotation,
        finalize: bool,
    ) -> Result<bool, Error> {
        self.behavior
            .finalize_template_entity(self, tag, mirror, rotation, finalize)
    }
    pub fn is_outline_full_block(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        self.behavior.is_outline_full_block(self, state, p)
    }
    pub fn is_collision_full_block(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        match state.cached_collision_full_block() {
            Some(full) => Ok(full),
            None => self.behavior.is_collision_full_block(self, state, p),
        }
    }
    pub fn is_solid(&self, p: [i32; 3]) -> Result<bool, Error> {
        Ok(self.block(p)?.is_solid())
    }
    pub fn has_sturdy_face(
        &self,
        p: [i32; 3],
        face: ferraloom_util::Direction,
    ) -> Result<bool, Error> {
        let state = self.block(p)?;
        match state.cached_sturdy_face(face) {
            Some(sturdy) => Ok(sturdy),
            None => self.behavior.has_sturdy_face(state, self, p, face),
        }
    }
    pub fn can_survive(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        self.behavior.can_survive(state, self, p)
    }
    pub fn is_unobstructed(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        self.behavior.is_unobstructed(state, self, p)
    }
}

/// The light layers queried by LevelReader during feature placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightLayer {
    Block,
    Sky,
}

impl WorldGenRegion<'_> {
    // --- vegetation family ---
    /// Port of BlockAndLightGetter#getBrightness before INITIALIZE_LIGHT.
    /// No section light exists yet: block light is zero and sky storage returns 15.
    /// A feature placing an emitter does not initialize or propagate lighting.
    pub fn brightness(&self, layer: LightLayer, p: [i32; 3]) -> Result<u8, Error> {
        let chunk = self.chunk([p[0] >> 4, p[2] >> 4], ChunkStatus::Empty)?;
        if chunk.status >= ChunkStatus::InitializeLight {
            return Err(Error(
                "Unsupported: initialized worldgen light storage".into(),
            ));
        }
        Ok(match layer {
            LightLayer::Block => 0,
            LightLayer::Sky => match self.has_skylight {
                Some(true) => 15,
                Some(false) => 0,
                None => return Err(Error("Unsupported: worldgen skylight context".into())),
            },
        })
    }
    pub fn set_has_skylight(&mut self, has_skylight: bool) {
        self.has_skylight = Some(has_skylight);
    }
    pub fn vine_acceptable_neighbor(
        &self,
        p: [i32; 3],
        direction: ferraloom_util::Direction,
    ) -> Result<bool, Error> {
        self.behavior.vine_acceptable_neighbor(self, p, direction)
    }
    pub fn place_double_plant(
        &mut self,
        state: BlockState,
        p: [i32; 3],
        flags: u32,
    ) -> Result<(), Error> {
        self.behavior.place_double_plant(self, state, p, flags)
    }
    pub fn place_mossy_carpet(&mut self, p: [i32; 3], flags: u32) -> Result<(), Error> {
        self.behavior.place_mossy_carpet(self, p, flags)
    }
    // --- end vegetation family ---
}
