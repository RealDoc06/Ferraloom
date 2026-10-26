//! PortalShape's frame search and construction. Entity portal travel is separate.
use crate::{
    block::{LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{Axis, BlockPos, Direction};

/// Port of PortalShape. The search retains coordinates and counts, never a
/// copied world; construction performs each native flags-18 write synchronously.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PortalShape {
    pub axis: Axis,
    pub bottom_left: BlockPos,
    pub width: i32,
    pub height: i32,
    pub portal_blocks: i32,
}

fn is_empty(state: BlockState, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    Ok(state.is_air()
        || level
            .block_tags()?
            .contains_key(BlockTags::FIRE, state.block())
        || state.block() == Block::NETHER_PORTAL)
}
fn is_frame(pos: BlockPos, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    Ok(level.block_tags()?.contains_key(
        BlockTags::NETHER_PORTAL_FRAME,
        level.get_block_state(pos)?.block(),
    ))
}
fn edge_distance(
    pos: BlockPos,
    direction: Direction,
    level: &LevelReaderRef<'_, '_>,
) -> Result<i32, LevelError> {
    for distance in 0..=21 {
        let at = pos.relative(direction, distance);
        if !is_empty(level.get_block_state(at)?, level)? {
            return Ok(if is_frame(at, level)? { distance } else { 0 });
        }
        if !is_frame(at.below(1), level)? {
            break;
        }
    }
    Ok(0)
}
impl PortalShape {
    fn right(self) -> Direction {
        if self.axis == Axis::X {
            Direction::West
        } else {
            Direction::South
        }
    }
    /// Port of PortalShape#findAnyShape, including invalid partial shapes.
    pub fn find_any(
        mut pos: BlockPos,
        axis: Axis,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<Self, LevelError> {
        let original = pos;
        let right = if axis == Axis::X {
            Direction::West
        } else {
            Direction::South
        };
        let min_y = level.min_build_height().max(pos.y - 21);
        while pos.y > min_y && is_empty(level.get_block_state(pos.below(1))?, level)? {
            pos = pos.below(1);
        }
        let edge = edge_distance(pos, right.opposite(), level)? - 1;
        let mut shape = Self {
            axis,
            bottom_left: original,
            width: 0,
            height: 0,
            portal_blocks: 0,
        };
        if edge < 0 {
            return Ok(shape);
        }
        shape.bottom_left = pos.relative(right.opposite(), edge);
        let width = edge_distance(shape.bottom_left, right, level)?;
        if !(2..=21).contains(&width) {
            return Ok(shape);
        }
        shape.width = width;
        let mut height = 21;
        'layers: for y in 0..21 {
            let row = shape.bottom_left.above(y);
            if !is_frame(row.relative(right, -1), level)?
                || !is_frame(row.relative(right, width), level)?
            {
                height = y;
                break;
            }
            for x in 0..width {
                let state = level.get_block_state(row.relative(right, x))?;
                if !is_empty(state, level)? {
                    height = y;
                    break 'layers;
                }
                if state.block() == Block::NETHER_PORTAL {
                    shape.portal_blocks += 1;
                }
            }
        }
        if (3..=21).contains(&height) {
            for x in 0..width {
                if !is_frame(shape.bottom_left.above(height).relative(right, x), level)? {
                    return Ok(shape);
                }
            }
            shape.height = height;
        }
        Ok(shape)
    }
    pub fn is_valid(self) -> bool {
        (2..=21).contains(&self.width) && (3..=21).contains(&self.height)
    }
    /// Port of PortalShape#findEmptyPortalShape: preferred axis first, then its
    /// horizontal alternative. Existing portal cells disqualify either shape.
    pub fn find_empty(
        pos: BlockPos,
        preferred: Axis,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<Option<Self>, LevelError> {
        for axis in [
            preferred,
            if preferred == Axis::X {
                Axis::Z
            } else {
                Axis::X
            },
        ] {
            let shape = Self::find_any(pos, axis, level)?;
            if shape.is_valid() && shape.portal_blocks == 0 {
                return Ok(Some(shape));
            }
        }
        Ok(None)
    }
    /// Port of PortalShape#createPortalBlocks. betweenClosed normalizes the
    /// corners and visits X fastest, then Y, then Z, including WEST-facing frames.
    pub fn create_portal_blocks(self, level: &mut Level) -> Result<(), LevelError> {
        let state = Block::NETHER_PORTAL
            .default_state()
            .with(
                p::AXIS,
                match self.axis {
                    Axis::X => p::Axis::X,
                    Axis::Z => p::Axis::Z,
                    Axis::Y => {
                        unreachable!("nether portal construction requires a horizontal axis")
                    }
                },
            )
            .unwrap();
        let end = self
            .bottom_left
            .above(self.height - 1)
            .relative(self.right(), self.width - 1);
        for pos in BlockPos::between_closed(self.bottom_left, end) {
            level.set_block(
                pos,
                state,
                UpdateFlags::CLIENTS | UpdateFlags::KNOWN_SHAPE,
                UPDATE_LIMIT,
            )?;
        }
        Ok(())
    }
}
