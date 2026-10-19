//! Port of PathNavigationRegion. Capture only nullable getChunkNow identities;
//! neighbor shape queries use this same bounded reader and never acquire chunks.
use super::{
    Level, LevelError, RegionId,
    acquisition::{ChunkKind, ChunkReference},
    chunk::LevelChunk,
};
use crate::{
    block::{
        BlockBehavior, BlockCollisionContext, BlockMethod, LevelReaderRef,
        entity::{BlockEntityDataView, BlockEntityRef, BlockEntityState},
    },
    entity::{EntityData, EntityRef, EntityState, collision_context::EntityCollisionView},
};
use ferraloom_data::{
    Block, BlockState, EntityType, FluidState, datapack::TagSet, registries::fluid,
};
use ferraloom_util::{Aabb, BlockPos, ChunkPos, Direction, VoxelShape, shapes};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathNavigationError {
    MissingOwner,
    WrongOwner {
        expected: RegionId,
        actual: RegionId,
    },
    StaleChunk(ChunkReference),
    InvalidBounds {
        start: BlockPos,
        end: BlockPos,
    },
}

/// These LevelReader extensions are not part of native BlockGetter or
/// CollisionGetter. A bounded path reader cannot borrow the live Level instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathNavigationQuery {
    Light,
    Environment,
    RetainedStack,
}

/// Port of PathNavigationRegion's X-major ChunkAccess array. Null entries stay
/// null even if chunks load later. No live blocks or Level borrows are copied.
pub struct PathNavigationRegion {
    owner: RegionId,
    min_chunk: ChunkPos,
    max_chunk: ChunkPos,
    z_len: usize,
    chunks: Vec<Option<ChunkReference>>,
    min_y: i32,
    height: i32,
    all_empty: bool,
}
impl PathNavigationRegion {
    pub fn capture(level: &Level, start: BlockPos, end: BlockPos) -> Result<Self, LevelError> {
        let owner = level.region.as_ref().ok_or(LevelError::PathNavigation(
            PathNavigationError::MissingOwner,
        ))?;
        let min_chunk = ChunkPos::containing(start);
        let max_chunk = ChunkPos::containing(end);
        if min_chunk.x > max_chunk.x || min_chunk.z > max_chunk.z {
            return Err(LevelError::PathNavigation(
                PathNavigationError::InvalidBounds { start, end },
            ));
        }
        let mut snapshot = Self {
            owner: owner.id,
            min_chunk,
            max_chunk,
            z_len: (max_chunk.z - min_chunk.z + 1) as usize,
            chunks: Vec::new(),
            min_y: owner.min_y,
            height: owner.max_y - owner.min_y,
            all_empty: true,
        };
        for x in min_chunk.x..=max_chunk.x {
            for z in min_chunk.z..=max_chunk.z {
                let pos = ChunkPos::new(x, z);
                let reference = level.get_chunk_now(pos)?.map(|chunk| ChunkReference {
                    pos,
                    generation: chunk.publication_generation,
                    kind: ChunkKind::Full,
                });
                snapshot.chunks.push(reference);
            }
        }
        let all_empty = {
            let reader = PathNavigationReader {
                region: &snapshot,
                level,
            };
            let mut all_empty = true;
            for reference in snapshot.chunks.iter().flatten() {
                let chunk = reader.resolve(*reference)?;
                // Port of ChunkAccess#isYSpaceEmpty: clamp, then step from the
                // supplied Y by 16, rather than rounding it to a section start.
                let mut y = start.y.max(snapshot.min_y);
                let max_y = end.y.min(snapshot.min_y + snapshot.height - 1);
                while y <= max_y {
                    if !chunk
                        .sections()
                        .nth(((y - snapshot.min_y) >> 4) as usize)
                        .expect("captured chunk height")
                        .has_only_air()
                    {
                        all_empty = false;
                        break;
                    }
                    y += 16;
                }
                if !all_empty {
                    break;
                }
            }
            all_empty
        };
        snapshot.all_empty = all_empty;
        Ok(snapshot)
    }

    /// Native creates and consumes this region in one synchronous createPath.
    /// Owned identities permit short Rust reads; reuse after unload/replacement
    /// is a required misuse error instead of silently reading a different chunk.
    pub fn reader<'call>(
        &'call self,
        level: &'call Level,
    ) -> Result<LevelReaderRef<'call, 'static>, LevelError> {
        let reader = PathNavigationReader {
            region: self,
            level,
        };
        reader.checked_level()?;
        Ok(LevelReaderRef::PathNavigation(reader))
    }
    pub fn captured_chunks(&self) -> &[Option<ChunkReference>] {
        &self.chunks
    }
    pub fn chunk_bounds(&self) -> (ChunkPos, ChunkPos) {
        (self.min_chunk, self.max_chunk)
    }
    pub fn min_y(&self) -> i32 {
        self.min_y
    }
    pub fn height(&self) -> i32 {
        self.height
    }
    pub fn all_empty(&self) -> bool {
        self.all_empty
    }
    fn reference(&self, pos: ChunkPos) -> Option<ChunkReference> {
        if pos.x < self.min_chunk.x
            || pos.x > self.max_chunk.x
            || pos.z < self.min_chunk.z
            || pos.z > self.max_chunk.z
        {
            return None;
        }
        self.chunks
            [(pos.x - self.min_chunk.x) as usize * self.z_len + (pos.z - self.min_chunk.z) as usize]
    }
}

/// Short read frame. Its fields are private so block families cannot escape the
/// snapshot by extracting a Level and constructing an unbounded Server reader.
#[derive(Clone, Copy)]
pub struct PathNavigationReader<'call> {
    region: &'call PathNavigationRegion,
    level: &'call Level,
}
impl<'call> PathNavigationReader<'call> {
    fn checked_level(&self) -> Result<&'call Level, LevelError> {
        let region = self
            .level
            .region
            .as_ref()
            .ok_or(LevelError::PathNavigation(
                PathNavigationError::MissingOwner,
            ))?;
        if region.id != self.region.owner {
            return Err(LevelError::PathNavigation(
                PathNavigationError::WrongOwner {
                    expected: self.region.owner,
                    actual: region.id,
                },
            ));
        }
        Ok(self.level)
    }
    fn resolve(&self, reference: ChunkReference) -> Result<&'call LevelChunk, LevelError> {
        self.checked_level()?
            .chunk(reference.pos)
            .filter(|chunk| {
                reference.kind == ChunkKind::Full
                    && chunk.publication_generation == reference.generation
            })
            .ok_or(LevelError::PathNavigation(PathNavigationError::StaleChunk(
                reference,
            )))
    }
    fn chunk(&self, pos: ChunkPos) -> Result<Option<&'call LevelChunk>, LevelError> {
        self.checked_level()?;
        self.region
            .reference(pos)
            .map(|reference| self.resolve(reference))
            .transpose()
    }
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        self.checked_level()?;
        if pos.y < self.region.min_y || pos.y >= self.region.min_y + self.region.height {
            return Ok(Block::AIR.default_state());
        }
        self.block_state_for_collisions(pos)
    }
    /// BlockCollisions reads captured ChunkAccess directly: an EmptyLevelChunk
    /// returns VOID_AIR even outside Y, while a real chunk returns AIR there.
    pub(crate) fn block_state_for_collisions(
        &self,
        pos: BlockPos,
    ) -> Result<BlockState, LevelError> {
        Ok(self
            .chunk(ChunkPos::containing(pos))?
            .map_or(Block::VOID_AIR.default_state(), |chunk| {
                chunk.get_block_state(pos)
            }))
    }
    pub fn get_fluid_state(&self, pos: BlockPos) -> Result<FluidState, LevelError> {
        Ok(self.get_block_state(pos)?.data().fluid)
    }
    pub fn min_build_height(&self) -> i32 {
        self.region.min_y
    }
    pub fn max_build_height(&self) -> i32 {
        self.region.min_y + self.region.height - 1
    }
    pub fn height(&self) -> i32 {
        self.region.height
    }
    pub fn validate_chunk(&self, pos: ChunkPos) -> Result<(), LevelError> {
        self.chunk(pos).map(|_| ())
    }
    pub fn block_tags(&self) -> Result<&'call TagSet<Block>, LevelError> {
        self.checked_level()?.block_tags()
    }
    pub fn fluid_tags(&self) -> Result<&'call TagSet<fluid::Id>, LevelError> {
        self.checked_level()?.fluid_tags()
    }
    pub(crate) fn required_behavior(
        &self,
        block: Block,
        method: BlockMethod,
    ) -> Result<&'static dyn BlockBehavior, LevelError> {
        self.checked_level()?.required_block_behavior(block, method)
    }
    pub fn block_entity_ref(&self, pos: BlockPos) -> Result<Option<BlockEntityRef>, LevelError> {
        let Some(chunk) = self.chunk(ChunkPos::containing(pos))? else {
            return Ok(None);
        };
        if let Some(state) = chunk.block_entities.get(pos) {
            return Ok(Some(state.data().reference()));
        }
        if chunk
            .pending_block_entity_nbt()
            .iter()
            .any(|tag| crate::block::entity::get_pos_from_tag(chunk.pos(), tag) == pos)
        {
            return Err(LevelError::UnsupportedBlockEntity(
                chunk.get_block_state(pos).block(),
            ));
        }
        Ok(None)
    }
    pub fn block_entity_state(
        &self,
        reference: BlockEntityRef,
    ) -> Result<&'call BlockEntityState, LevelError> {
        self.chunk(ChunkPos::containing(reference.pos))?
            .and_then(|chunk| chunk.block_entities.get(reference.pos))
            .filter(|state| state.data().generation == reference.generation)
            .ok_or(LevelError::MissingBlockEntity(reference))
    }
    pub fn block_entity<T: BlockEntityDataView>(
        &self,
        reference: BlockEntityRef,
    ) -> Result<&'call T, LevelError> {
        T::view(self.block_entity_state(reference)?)
            .ok_or(LevelError::WrongBlockEntityType(reference))
    }
    pub(crate) fn entity_state(&self, entity: EntityRef) -> Result<&'call EntityState, LevelError> {
        self.checked_level()?.entity_state(entity)
    }
    pub(crate) fn entity_type_tags(&self) -> Result<&'call TagSet<EntityType>, LevelError> {
        self.checked_level()?.entity_type_tags()
    }
    pub(crate) fn wire_signals_enabled(&self) -> bool {
        !self.level.region().redstone.wire_signals_suppressed
    }
    pub(crate) fn piston_noclip(&self) -> Result<Option<Direction>, LevelError> {
        Ok(self.checked_level()?.region().redstone.piston_noclip)
    }
    pub(crate) fn liquid_collision_shape(
        &self,
        entity: &EntityCollisionView,
    ) -> Result<Option<VoxelShape>, LevelError> {
        entity.liquid_collision_shape(self.checked_level()?)
    }
    pub(crate) fn collision_context(
        &self,
        entity: Option<EntityRef>,
        liquids: bool,
    ) -> Result<BlockCollisionContext<'static>, LevelError> {
        let level = self.checked_level()?;
        Ok(match entity {
            Some(entity) => {
                BlockCollisionContext::Entity(EntityCollisionView::capture(entity, level, liquids)?)
            }
            None => BlockCollisionContext::empty_with_fluid_collisions(liquids),
        })
    }
    /// Port of PathNavigationRegion#getEntityCollisions: always empty.
    pub fn get_entity_collisions(
        &self,
        _source: Option<EntityRef>,
        _area: Aabb,
    ) -> Result<Vec<VoxelShape>, LevelError> {
        self.checked_level()?;
        Ok(Vec::new())
    }
    /// Port of CollisionGetter#noCollision over the region. Native ordering is
    /// block collisions, empty entity collisions, then the live world border.
    pub fn no_collision(&self, source: Option<EntityRef>, area: Aabb) -> Result<bool, LevelError> {
        let context = self.collision_context(source, false)?;
        if crate::entity::collision::visit_block_collisions(
            &LevelReaderRef::PathNavigation(*self),
            &context,
            area,
            |_, _| std::ops::ControlFlow::Break(()),
        )?
        .is_break()
        {
            return Ok(false);
        }
        if let Some(entity) = source {
            let level = self.checked_level()?;
            let border = level.entity_world_border();
            if border.is_inside_close_to_border(level.entity::<EntityData>(entity)?.position, area)
                && shapes::join_is_not_empty(
                    &border.collision_shape(),
                    &shapes::create_aabb(area),
                    shapes::BooleanOp::And,
                )
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
