//! Ports of NeighborUpdater, CollectingNeighborUpdater and Orientation.
use super::{Level, LevelError};
use crate::block::{BlockMethod, LevelAccessRef, UpdateFlags};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{
    BlockPos, Direction,
    core::{Axis, AxisDirection},
    random::RandomSource,
};

pub const UPDATE_ORDER: [Direction; 6] = [
    Direction::West,
    Direction::East,
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
];
pub const UPDATE_SHAPE_ORDER: [Direction; 6] = [
    Direction::West,
    Direction::East,
    Direction::North,
    Direction::South,
    Direction::Down,
    Direction::Up,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SideBias {
    Left,
    Right,
}
/// Port of Orientation. Only perpendicular up/front pairs are constructible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Orientation {
    up: Direction,
    front: Direction,
    bias: SideBias,
}
impl Orientation {
    pub fn new(up: Direction, front: Direction, bias: SideBias) -> Option<Self> {
        (up.axis() != front.axis()).then_some(Self { up, front, bias })
    }
    pub fn up(self) -> Direction {
        self.up
    }
    pub fn front(self) -> Direction {
        self.front
    }
    pub fn side_bias(self) -> SideBias {
        self.bias
    }
    pub fn with_front(self, front: Direction) -> Self {
        let up = if front == self.up {
            self.front.opposite()
        } else if front == self.up.opposite() {
            self.front
        } else {
            self.up
        };
        Self { up, front, ..self }
    }
    pub fn with_up(self, up: Direction) -> Self {
        let front = if up == self.front {
            self.up.opposite()
        } else if up == self.front.opposite() {
            self.up
        } else {
            self.front
        };
        Self { up, front, ..self }
    }
    pub fn with_side_bias(self, bias: SideBias) -> Self {
        Self { bias, ..self }
    }
    pub fn index(self) -> u8 {
        let front_axis_key = if self.up.axis() == Axis::Y {
            self.front.axis() == Axis::X
        } else {
            self.front.axis() == Axis::Y
        };
        (((self.up as u8 * 4)
            + (u8::from(front_axis_key) * 2
                | u8::from(self.front.axis_direction() == AxisDirection::Negative)))
            * 2)
            + u8::from(self.bias == SideBias::Right)
    }
    pub fn from_index(index: u8) -> Option<Self> {
        if index >= 48 {
            return None;
        }
        let up = Direction::VALUES[(index >> 3) as usize];
        let axis_key = index & 4 != 0;
        let axis = match up.axis() {
            Axis::Y => {
                if axis_key {
                    Axis::X
                } else {
                    Axis::Z
                }
            }
            Axis::X => {
                if axis_key {
                    Axis::Y
                } else {
                    Axis::Z
                }
            }
            Axis::Z => {
                if axis_key {
                    Axis::Y
                } else {
                    Axis::X
                }
            }
        };
        Self::new(
            up,
            if index & 2 == 0 {
                axis.positive()
            } else {
                axis.negative()
            },
            if index & 1 == 0 {
                SideBias::Left
            } else {
                SideBias::Right
            },
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum NeighborUpdate {
    Changed {
        state: Option<BlockState>,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        moved: bool,
    },
    Shape {
        direction: Direction,
        neighbor: BlockState,
        pos: BlockPos,
        neighbor_pos: BlockPos,
        flags: UpdateFlags,
        limit: i32,
    },
    Multi {
        source: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        skip: Option<Direction>,
        index: usize,
    },
}
/// The active operation is represented by handles/scalars; no chunk borrow
/// survives a callback. Newly added operations retain call order within a layer.
pub(super) struct CollectingNeighborUpdater {
    stack: Vec<NeighborUpdate>,
    added_this_layer: Vec<NeighborUpdate>,
    count: i32,
    pub max_chained_updates: i32,
    pub first_skipped: Option<BlockPos>,
}
impl Default for CollectingNeighborUpdater {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            added_this_layer: Vec::new(),
            count: 0,
            max_chained_updates: 1_000_000,
            first_skipped: None,
        }
    }
}
impl Level {
    pub fn set_max_chained_neighbor_updates(&mut self, max: i32) {
        self.region_mut().neighbor_updater.max_chained_updates = max;
    }
    pub fn first_skipped_neighbor_update(&self) -> Option<BlockPos> {
        self.region().neighbor_updater.first_skipped
    }
    fn initial_orientation(&mut self, front: Option<Direction>) -> Option<Orientation> {
        if !self.rules().redstone_experiments {
            return None;
        }
        let index = self.random().next_int_bound(48) as u8;
        let orientation = Orientation::from_index(index)
            .unwrap()
            .with_side_bias(SideBias::Left);
        Some(front.map_or(orientation, |front| orientation.with_front(front)))
    }
    pub fn neighbor_changed(
        &mut self,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
    ) -> Result<(), LevelError> {
        self.add_neighbor_update(
            pos,
            NeighborUpdate::Changed {
                state: None,
                pos,
                block,
                orientation,
                moved: false,
            },
        )
    }
    pub fn neighbor_changed_with_state(
        &mut self,
        state: BlockState,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        moved: bool,
    ) -> Result<(), LevelError> {
        self.add_neighbor_update(
            pos,
            NeighborUpdate::Changed {
                state: Some(state),
                pos,
                block,
                orientation,
                moved,
            },
        )
    }
    pub fn update_neighbors_at(&mut self, pos: BlockPos, block: Block) -> Result<(), LevelError> {
        let orientation = self.initial_orientation(None);
        self.update_neighbors_at_except_from_facing(pos, block, None, orientation)
    }
    pub fn update_neighbors_at_except_from_facing(
        &mut self,
        pos: BlockPos,
        block: Block,
        skip: Option<Direction>,
        orientation: Option<Orientation>,
    ) -> Result<(), LevelError> {
        let index = usize::from(skip == Some(UPDATE_ORDER[0]));
        self.add_neighbor_update(
            pos,
            NeighborUpdate::Multi {
                source: pos,
                block,
                orientation,
                skip,
                index,
            },
        )
    }
    pub fn neighbor_shape_changed(
        &mut self,
        direction: Direction,
        pos: BlockPos,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        flags: UpdateFlags,
        limit: i32,
    ) -> Result<(), LevelError> {
        self.add_neighbor_update(
            pos,
            NeighborUpdate::Shape {
                direction,
                neighbor,
                pos,
                neighbor_pos,
                flags,
                limit,
            },
        )
    }
    fn add_neighbor_update(
        &mut self,
        pos: BlockPos,
        update: NeighborUpdate,
    ) -> Result<(), LevelError> {
        let updater = &mut self.region_mut().neighbor_updater;
        let running = updater.count > 0;
        let excess =
            updater.max_chained_updates >= 0 && updater.count >= updater.max_chained_updates;
        if excess && updater.count == updater.max_chained_updates {
            updater.first_skipped = Some(pos);
            eprintln!(
                "Too many chained neighbor updates. Skipping the rest. First skipped position: {}, {}, {}",
                pos.x, pos.y, pos.z
            );
        }
        updater.count = updater.count.wrapping_add(1);
        if !excess {
            if running {
                updater.added_this_layer.push(update);
            } else {
                updater.stack.push(update);
            }
        }
        if running {
            return Ok(());
        }
        let result = self.run_neighbor_updates();
        let updater = &mut self.region_mut().neighbor_updater;
        updater.stack.clear();
        updater.added_this_layer.clear();
        updater.count = 0;
        result
    }
    fn run_neighbor_updates(&mut self) -> Result<(), LevelError> {
        loop {
            let updater = &mut self.region_mut().neighbor_updater;
            for update in updater.added_this_layer.drain(..).rev() {
                updater.stack.push(update);
            }
            let Some(update) = updater.stack.pop() else {
                return Ok(());
            };
            match update {
                NeighborUpdate::Changed {
                    state,
                    pos,
                    block,
                    orientation,
                    moved,
                } => {
                    let state = match state {
                        Some(state) => state,
                        None => self.get_block_state(pos)?,
                    };
                    self.optional_block_call(
                        state.block(),
                        BlockMethod::NeighborChanged,
                        |behavior, level| {
                            behavior.neighbor_changed(state, pos, block, orientation, moved, level)
                        },
                    )?;
                }
                NeighborUpdate::Multi {
                    source,
                    block,
                    mut orientation,
                    skip,
                    mut index,
                } => {
                    let direction = UPDATE_ORDER[index];
                    index += 1;
                    let pos = source.relative(direction, 1);
                    let state = self.get_block_state(pos)?;
                    let delivered = if self.rules().redstone_experiments {
                        if orientation.is_none() {
                            orientation = self.initial_orientation(skip.map(Direction::opposite));
                        }
                        orientation.map(|o| o.with_front(direction))
                    } else {
                        None
                    };
                    if index < 6 && skip == Some(UPDATE_ORDER[index]) {
                        index += 1;
                    }
                    if index < 6 {
                        self.region_mut()
                            .neighbor_updater
                            .stack
                            .push(NeighborUpdate::Multi {
                                source,
                                block,
                                orientation,
                                skip,
                                index,
                            });
                    }
                    self.optional_block_call(
                        state.block(),
                        BlockMethod::NeighborChanged,
                        |behavior, level| {
                            behavior.neighbor_changed(state, pos, block, delivered, false, level)
                        },
                    )?;
                }
                NeighborUpdate::Shape {
                    direction,
                    neighbor,
                    pos,
                    neighbor_pos,
                    flags,
                    limit,
                } => {
                    let current = self.get_block_state(pos)?;
                    if flags.contains(UpdateFlags::SKIP_SHAPE_UPDATE_ON_WIRE)
                        && current.block() == Block::REDSTONE_WIRE
                    {
                        continue;
                    }
                    let new = self.optional_block_call(
                        current.block(),
                        BlockMethod::UpdateShape,
                        |behavior, level| {
                            behavior.update_shape(
                                current,
                                pos,
                                direction,
                                neighbor_pos,
                                neighbor,
                                &mut LevelAccessRef::Server(level),
                            )
                        },
                    )?;
                    if let Some(new) = new {
                        self.update_or_destroy(current, new, pos, flags, limit)?;
                    }
                }
            }
        }
    }
    /// Port of Level#updateNeighbourForOutputSignal. The first hasChunkAt
    /// guard is vanilla's explicit non-loading probe; the second read through
    /// a conductor remains a synchronous required read (MissingChunk for now).
    pub fn update_neighbour_for_output_signal(
        &mut self,
        pos: BlockPos,
        changed_block: Block,
    ) -> Result<(), LevelError> {
        for direction in ferraloom_util::core::Plane::Horizontal {
            let mut target = pos.relative(direction, 1);
            if self
                .chunk(ferraloom_util::ChunkPos::containing(target))
                .is_none()
            {
                continue;
            }
            let mut state = self.get_block_state(target)?;
            if state.block() == Block::COMPARATOR {
                self.neighbor_changed_with_state(state, target, changed_block, None, false)?;
            } else if state
                .cached_redstone_conductor()
                .ok_or(LevelError::UnsupportedShape(state))?
            {
                target = target.relative(direction, 1);
                state = self.get_block_state(target)?;
                if state.block() == Block::COMPARATOR {
                    self.neighbor_changed_with_state(state, target, changed_block, None, false)?;
                }
            }
        }
        Ok(())
    }
    pub fn update_neighbor_shapes(
        &mut self,
        state: BlockState,
        pos: BlockPos,
        flags: UpdateFlags,
        limit: i32,
    ) -> Result<(), LevelError> {
        for direction in UPDATE_SHAPE_ORDER {
            self.neighbor_shape_changed(
                direction.opposite(),
                pos.relative(direction, 1),
                pos,
                state,
                flags,
                limit,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::{LegacyRandomSource, Random};

    #[test]
    fn orientation_indices_transforms_and_initial_random_draw_match_vanilla() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        for line in include_str!("../../tests/java/level/orientation.tsv").lines() {
            let (case, expected) = line.split_once('\t').unwrap();
            if let Some(index) = case.strip_prefix("orientation:") {
                let index = index.parse::<u8>().unwrap();
                let orientation = Orientation::from_index(index).unwrap();
                assert_eq!(orientation.index(), index);
                let indices = |transform: fn(Orientation, Direction) -> Orientation| {
                    Direction::VALUES
                        .map(|direction| transform(orientation, direction).index().to_string())
                        .join(",")
                };
                assert_eq!(
                    format!(
                        "{}:{}:{}|{}|{}",
                        orientation.up() as u8,
                        orientation.front() as u8,
                        u8::from(orientation.side_bias() == SideBias::Right),
                        indices(Orientation::with_front),
                        indices(Orientation::with_up)
                    ),
                    expected,
                    "{case}"
                );
            } else {
                let (experiments, front) = case
                    .strip_prefix("initial-orientation:")
                    .unwrap()
                    .split_once(':')
                    .unwrap();
                let front = front.parse::<i32>().unwrap();
                let front = if front < 0 {
                    None
                } else {
                    Some(Direction::VALUES[front as usize])
                };
                let mut level = Level::new(super::super::Region::new(&BEHAVIORS));
                level.set_random_streams(Random::Legacy(LegacyRandomSource::new(0)), 0);
                level.rules_mut().redstone_experiments = experiments.parse().unwrap();
                let orientation = level
                    .initial_orientation(front)
                    .map_or(-1, |orientation| i32::from(orientation.index()));
                assert_eq!(
                    format!("{orientation}:{}", level.random().next_int()),
                    expected,
                    "{case}"
                );
            }
        }
    }
}
