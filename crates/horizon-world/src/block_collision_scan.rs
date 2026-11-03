//! Lazy block collision selection in vanilla cursor/query order. Providers own
//! chunk availability, state metadata, suffocation predicates and context dispatch.
use crate::{collision_context::CollisionContext, registry::BlockStateId};
use horizon_core::{
    geometry::{Aabb, ShapeError, ShapeOp, Vec3, VoxelShape},
    position::{BlockPos, ChunkPos},
};

#[derive(Clone, Copy, Debug)]
pub struct CollisionBlock {
    pub state: BlockStateId,
    pub large_collision_shape: bool,
    pub moving_piston: bool,
    /// The shape is empty under every context (air, plants): a scan skips
    /// the block without asking for its shape. Providers whose shapes come
    /// from elsewhere leave it false.
    pub empty_shape: bool,
}

pub trait BlockCollisionWorld {
    type Error;
    type Chunk: Copy;
    /// None is a missing collision chunk; native iteration retries missing chunks.
    fn collision_chunk(&mut self, position: ChunkPos) -> Result<Option<Self::Chunk>, Self::Error>;
    /// Select a section for subsequent reads. Providers may return a borrowed
    /// section in their chunk handle; the default keeps chunk-only providers.
    fn collision_section(&mut self, chunk: Self::Chunk, _section_y: i32) -> Self::Chunk {
        chunk
    }
    fn collision_block(
        &mut self,
        chunk: Self::Chunk,
        position: BlockPos,
    ) -> Result<CollisionBlock, Self::Error>;
    fn is_suffocating(
        &mut self,
        chunk: Self::Chunk,
        block: CollisionBlock,
        position: BlockPos,
    ) -> Result<bool, Self::Error>;
    /// Query against the global collision world, not the cached chunk provider.
    /// Shapes must preserve canonical block identity (including imported shapes).
    fn context_shape(
        &mut self,
        block: CollisionBlock,
        position: BlockPos,
        context: &CollisionContext<'_>,
    ) -> Result<VoxelShape, Self::Error>;
    /// Whether a block of the chunk's section `section_y` (block y >> 4) may
    /// have a large collision shape or be a moving piston: the only blocks
    /// the scan keeps outside the box's own cells. False lets the scan pass
    /// that section's outer cells without reading them; true reads them all.
    fn may_reach_out(&mut self, _chunk: Self::Chunk, _section_y: i32) -> Result<bool, Self::Error> {
        Ok(true)
    }
    /// Whether every block of the section is `CollisionBlock::empty_shape`,
    /// so a scan keeps none of them. True lets the scan pass the section's
    /// cells without reading them.
    fn section_is_empty(
        &mut self,
        _chunk: Self::Chunk,
        _section_y: i32,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

/// What a section's palette says of the cells a scan visits in it.
#[derive(Clone, Copy, PartialEq)]
enum SectionCells {
    /// None can be kept.
    Empty,
    /// Only those in the box's own cells can be kept.
    InsideOnly,
    /// Any can be kept.
    All,
}

#[derive(Debug)]
pub enum CollisionScanError<E> {
    InvalidBounds,
    TooLarge,
    VisitBudget,
    World(E),
    Shape(ShapeError),
}
impl<E: std::fmt::Display> std::fmt::Display for CollisionScanError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBounds => f.write_str("collision scan bounds must be finite"),
            Self::TooLarge => {
                f.write_str("collision scan exceeds the positive Java-int cursor domain")
            }
            Self::VisitBudget => f.write_str("collision scan candidate budget exhausted"),
            Self::World(error) => error.fmt(f),
            Self::Shape(error) => error.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for CollisionScanError<E> {}

#[derive(Debug)]
pub struct BlockCollisionHit {
    pub position: BlockPos,
    pub shape: VoxelShape,
}

pub struct BlockCollisionScan<'w, 'c, W: BlockCollisionWorld> {
    world: &'w mut W,
    context: CollisionContext<'c>,
    bounds: Aabb,
    /// `Shapes.create(box)`, built the first time a block's shape is not
    /// the unit cube (finite bounds always make one).
    entity_shape: Option<VoxelShape>,
    only_suffocating: bool,
    start: BlockPos,
    sizes: [usize; 3],
    index: usize,
    /// The cell at `index`, relative to `start` (x fastest, then y, then z).
    cursor: [usize; 3],
    count: usize,
    budget: usize,
    chunk: Option<(ChunkPos, W::Chunk)>,
    section: Option<(ChunkPos, i32, W::Chunk)>,
    /// `SectionCells` of the last section visited.
    cells: Option<(ChunkPos, i32, SectionCells)>,
    failed: bool,
}
impl<'w, 'c, W: BlockCollisionWorld> BlockCollisionScan<'w, 'c, W> {
    /// Finite bounds and a positive-Java-int candidate volume are supported.
    /// Budget counts every cursor cell, including skipped corners. After an error
    /// iteration terminates; already performed queries are not rolled back.
    pub fn new(
        world: &'w mut W,
        context: CollisionContext<'c>,
        bounds: Aabb,
        only_suffocating: bool,
        candidate_budget: usize,
    ) -> Result<Self, CollisionScanError<W::Error>> {
        let min = bounds.min_position();
        let max = bounds.max_position();
        if !min.is_finite() || !max.is_finite() {
            return Err(CollisionScanError::InvalidBounds);
        }
        let start = BlockPos::containing(Vec3::new(min.x - 1.0e-7, min.y - 1.0e-7, min.z - 1.0e-7))
            .offset(-1, -1, -1);
        let end = BlockPos::containing(Vec3::new(max.x + 1.0e-7, max.y + 1.0e-7, max.z + 1.0e-7))
            .offset(1, 1, 1);
        let sizes = [
            end.x.wrapping_sub(start.x).wrapping_add(1),
            end.y.wrapping_sub(start.y).wrapping_add(1),
            end.z.wrapping_sub(start.z).wrapping_add(1),
        ];
        if sizes.iter().any(|size| *size <= 0) {
            return Err(CollisionScanError::TooLarge);
        }
        let count = sizes
            .into_iter()
            .try_fold(1u64, |n, size| {
                n.checked_mul(size as u64).filter(|n| *n <= i32::MAX as u64)
            })
            .ok_or(CollisionScanError::TooLarge)? as usize;
        Ok(Self {
            world,
            context,
            bounds,
            entity_shape: None,
            only_suffocating,
            start,
            sizes: sizes.map(|s| s as usize),
            index: 0,
            cursor: [0; 3],
            count,
            budget: candidate_budget,
            chunk: None,
            cells: None,
            section: None,
            failed: false,
        })
    }

    /// Move the cursor `n` cells further along its row; `n` must not pass
    /// the row's end.
    fn skip(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let [width, height, _] = self.sizes;
        self.index += n;
        self.cursor[0] += n;
        if self.cursor[0] == width {
            self.cursor[0] = 0;
            self.cursor[1] += 1;
            if self.cursor[1] == height {
                self.cursor[1] = 0;
                self.cursor[2] += 1;
            }
        }
    }

    fn find_next(&mut self) -> Result<Option<BlockCollisionHit>, CollisionScanError<W::Error>> {
        while self.index < self.count {
            if self.index >= self.budget {
                return Err(CollisionScanError::VisitBudget);
            }
            let [width, height, depth] = self.sizes;
            let relative = self.cursor;
            self.index += 1;
            self.cursor[0] += 1;
            if self.cursor[0] == width {
                self.cursor[0] = 0;
                self.cursor[1] += 1;
                if self.cursor[1] == height {
                    self.cursor[1] = 0;
                    self.cursor[2] += 1;
                }
            }
            let kind = usize::from(relative[0] == 0 || relative[0] == width - 1)
                + usize::from(relative[1] == 0 || relative[1] == height - 1)
                + usize::from(relative[2] == 0 || relative[2] == depth - 1);
            if kind == 3 {
                continue;
            }
            let p = self
                .start
                .offset(relative[0] as i32, relative[1] as i32, relative[2] as i32);
            let chunk_position = p.chunk();
            let chunk = match self.chunk {
                Some((at, chunk)) if at == chunk_position => Some(chunk),
                _ => {
                    self.section = None;
                    let chunk = self
                        .world
                        .collision_chunk(chunk_position)
                        .map_err(CollisionScanError::World)?;
                    self.chunk = chunk.map(|chunk| (chunk_position, chunk));
                    chunk
                }
            };
            let Some(chunk) = chunk else {
                continue;
            };
            let section_y = p.y >> 4;
            let chunk = match self.section {
                Some((at, y, section)) if at == chunk_position && y == section_y => section,
                _ => {
                    let section = self.world.collision_section(chunk, section_y);
                    self.section = Some((chunk_position, section_y, section));
                    section
                }
            };
            // An outer cell only counts for a block reaching out of its own
            // cell: most sections hold none, so their outer cells go unread
            // (the result is the same, with a fifth of the block reads for a
            // mob's box). A section of air and the like needs no cell read.
            let cells = match self.cells {
                Some((at, y, cells)) if at == chunk_position && y == section_y => cells,
                _ => {
                    let cells = if !self.only_suffocating
                        && self
                            .world
                            .section_is_empty(chunk, section_y)
                            .map_err(CollisionScanError::World)?
                    {
                        SectionCells::Empty
                    } else if self
                        .world
                        .may_reach_out(chunk, section_y)
                        .map_err(CollisionScanError::World)?
                    {
                        SectionCells::All
                    } else {
                        SectionCells::InsideOnly
                    };
                    self.cells = Some((chunk_position, section_y, cells));
                    cells
                }
            };
            let row_passes = cells == SectionCells::Empty
                || cells == SectionCells::InsideOnly
                    && kind != 0
                    // On a row along the box's bottom, top or sides every
                    // cell is outer.
                    && (relative[1] == 0
                        || relative[1] == height - 1
                        || relative[2] == 0
                        || relative[2] == depth - 1);
            if row_passes {
                // The rest of the row in this chunk is passed at once (never
                // beyond the budget, which still ends the scan where it would
                // have).
                let rest = (width - 1 - relative[0])
                    .min(15 - (p.x & 15) as usize)
                    .min(self.budget.saturating_sub(self.index));
                self.skip(rest);
                continue;
            }
            if cells == SectionCells::InsideOnly && kind != 0 {
                continue;
            }
            let block = self
                .world
                .collision_block(chunk, p)
                .map_err(CollisionScanError::World)?;
            if self.only_suffocating
                && !self
                    .world
                    .is_suffocating(chunk, block, p)
                    .map_err(CollisionScanError::World)?
            {
                continue;
            }
            if kind == 1 && !block.large_collision_shape || kind == 2 && !block.moving_piston {
                continue;
            }
            if block.empty_shape {
                continue;
            }
            let shape = self
                .world
                .context_shape(block, p, &self.context)
                .map_err(CollisionScanError::World)?;
            let offset = Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z));
            let moved = if shape.is_block() {
                // Native adds one in floating point, including at i32::MAX.
                let top = Vec3::new(offset.x + 1.0, offset.y + 1.0, offset.z + 1.0);
                if !self.bounds.intersects_corners(offset, top) {
                    continue;
                }
                shape.moved(offset)
            } else {
                // Shapes whose bounds miss the box share no volume with it:
                // joinIsNotEmpty(AND) is false without moving them.
                if !shape
                    .bounds()
                    .is_some_and(|b| b.move_by(offset).intersects(self.bounds))
                {
                    continue;
                }
                let moved = shape.moved(offset);
                let entity_shape = match &self.entity_shape {
                    Some(shape) => shape,
                    None => self.entity_shape.insert(
                        VoxelShape::from_box(self.bounds).map_err(CollisionScanError::Shape)?,
                    ),
                };
                if moved.is_empty()
                    || !moved
                        .join_is_not_empty(entity_shape, ShapeOp::AND)
                        .map_err(CollisionScanError::Shape)?
                {
                    continue;
                }
                moved
            };
            return Ok(Some(BlockCollisionHit {
                position: p,
                shape: moved,
            }));
        }
        Ok(None)
    }
}
impl<W: BlockCollisionWorld> Iterator for BlockCollisionScan<'_, '_, W> {
    type Item = Result<BlockCollisionHit, CollisionScanError<W::Error>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        match self.find_next() {
            Ok(hit) => hit.map(Ok),
            Err(error) => {
                self.failed = true;
                Some(Err(error))
            }
        }
    }
}
impl<W: BlockCollisionWorld> std::iter::FusedIterator for BlockCollisionScan<'_, '_, W> {}
