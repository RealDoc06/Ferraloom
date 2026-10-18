//! RedstoneWireBlock and both vanilla 26.3 wire evaluators.
use super::state::{HORIZONTAL, RedstoneSide, side, with_side};
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{
        Level, LevelError,
        redstone::{Orientation, SideBias},
    },
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_util::{
    BlockPos, Direction,
    collections::{JavaHashSet, Object2IntLinkedOpenHashMap},
    random::RandomSource,
};
use std::collections::VecDeque;

pub struct RedstoneWireBlock;
pub static WIRE: RedstoneWireBlock = RedstoneWireBlock;
fn is_dot(state: BlockState) -> bool {
    HORIZONTAL.into_iter().all(|d| !side(state, d).connected())
}
fn is_cross(state: BlockState) -> bool {
    HORIZONTAL.into_iter().all(|d| side(state, d).connected())
}
fn cross_state() -> BlockState {
    HORIZONTAL
        .into_iter()
        .fold(Block::REDSTONE_WIRE.default_state(), |s, d| {
            with_side(s, d, RedstoneSide::Side)
        })
}
fn survives_on(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    state: BlockState,
) -> Result<bool, LevelError> {
    Ok(level.is_face_sturdy(state, pos, Direction::Up)? || state.block() == Block::HOPPER)
}
fn connecting_side(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    direction: Direction,
    can_connect_up: bool,
) -> Result<RedstoneSide, LevelError> {
    let relative = pos.relative(direction, 1);
    let state = level.get_block_state(relative)?;
    if can_connect_up {
        let placeable = state.block().is_instance_of(BlockClass::TrapDoorBlock)
            || survives_on(level, relative, state)?;
        if placeable {
            let above = relative.above(1);
            if level.should_redstone_wire_connect_to(level.get_block_state(above)?, above, None)? {
                return Ok(
                    if level.is_face_sturdy(state, relative, direction.opposite())? {
                        RedstoneSide::Up
                    } else {
                        RedstoneSide::Side
                    },
                );
            }
        }
    }
    if !level.should_redstone_wire_connect_to(state, relative, Some(direction))? {
        if level.is_redstone_conductor(state, relative)? {
            return Ok(RedstoneSide::None);
        }
        let below = relative.below(1);
        if !level.should_redstone_wire_connect_to(level.get_block_state(below)?, below, None)? {
            return Ok(RedstoneSide::None);
        }
    }
    Ok(RedstoneSide::Side)
}
/// Port of RedstoneWireBlock#getConnectionState, including dot preservation and
/// the fallback pair of arms when only one axis has actual connections.
pub fn connection_state(
    level: &LevelReaderRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
) -> Result<BlockState, LevelError> {
    let was_dot = is_dot(state);
    let mut result = Block::REDSTONE_WIRE
        .default_state()
        .with(p::POWER, state.get(p::POWER).unwrap())
        .unwrap();
    // Vanilla passes pos here, even though it reads the block above pos.
    let can_up = !level.is_redstone_conductor(level.get_block_state(pos.above(1))?, pos)?;
    for direction in HORIZONTAL {
        result = with_side(
            result,
            direction,
            connecting_side(level, pos, direction, can_up)?,
        );
    }
    if was_dot && is_dot(result) {
        return Ok(result);
    }
    let ns_empty =
        !side(result, Direction::North).connected() && !side(result, Direction::South).connected();
    let ew_empty =
        !side(result, Direction::East).connected() && !side(result, Direction::West).connected();
    for (direction, empty) in [
        (Direction::West, ns_empty),
        (Direction::East, ns_empty),
        (Direction::North, ew_empty),
        (Direction::South, ew_empty),
    ] {
        if !side(result, direction).connected() && empty {
            result = with_side(result, direction, RedstoneSide::Side);
        }
    }
    Ok(result)
}
fn wire_power(state: BlockState) -> u8 {
    if state.block() == Block::REDSTONE_WIRE {
        state.get(p::POWER).unwrap()
    } else {
        0
    }
}
/// Port of RedstoneWireEvaluator#getIncomingWireSignal. The getter selects the
/// experimental evaluator's pending power without changing its read ordering.
fn incoming_wire_signal(
    level: &Level,
    pos: BlockPos,
    power: impl Fn(BlockPos, BlockState) -> u8,
) -> Result<i32, LevelError> {
    let mut best = 0_u8;
    for direction in HORIZONTAL {
        let neighbor_pos = pos.relative(direction, 1);
        let neighbor = level.get_block_state(neighbor_pos)?;
        best = best.max(power(neighbor_pos, neighbor));
        if level.is_redstone_conductor(neighbor, neighbor_pos)?
            && !level.is_redstone_conductor(level.get_block_state(pos.above(1))?, pos.above(1))?
        {
            let above = neighbor_pos.above(1);
            best = best.max(power(above, level.get_block_state(above)?));
        } else if !level.is_redstone_conductor(neighbor, neighbor_pos)? {
            let below = neighbor_pos.below(1);
            best = best.max(power(below, level.get_block_state(below)?));
        }
    }
    Ok(i32::from(best.saturating_sub(1)))
}
/// Port of RedstoneWireBlock#getBlockSignal. Only the external power query is
/// suppressed; pending wire propagation still reads stored wire strengths.
pub fn block_signal(level: &mut Level, pos: BlockPos) -> Result<i32, LevelError> {
    level.region_mut().redstone.wire_signals_suppressed = true;
    let result = level.get_best_neighbor_signal(pos);
    level.region_mut().redstone.wire_signals_suppressed = false;
    result
}
fn default_update(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let external = block_signal(level, pos)?;
    let target = if external == 15 {
        external
    } else {
        external.max(incoming_wire_signal(level, pos, |_, s| wire_power(s))?)
    };
    if i32::from(wire_power(state)) != target {
        if level.get_block_state(pos)? == state {
            level.set_block(
                pos,
                state
                    .with(p::POWER, u8::try_from(target).expect("wire power range"))
                    .unwrap(),
                UpdateFlags::CLIENTS,
                UPDATE_LIMIT,
            )?;
        }
        let mut updates = JavaHashSet::<BlockPos>::new();
        updates.insert(pos);
        for direction in Direction::VALUES {
            updates.insert(pos.relative(direction, 1));
        }
        for update in updates {
            level.update_neighbors_at(update, Block::REDSTONE_WIRE)?;
        }
    }
    Ok(())
}
fn update_power(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    orientation: Option<Orientation>,
    shape_initial: bool,
) -> Result<(), LevelError> {
    if level.rules().redstone_experiments {
        ExperimentalRedstoneWireEvaluator::default().update(level, pos, orientation, shape_initial)
    } else {
        default_update(level, pos, state)
    }
}
fn check_corner(level: &mut Level, pos: BlockPos) -> Result<(), LevelError> {
    if level.get_block_state(pos)?.block() == Block::REDSTONE_WIRE {
        level.update_neighbors_at(pos, Block::REDSTONE_WIRE)?;
        for direction in Direction::VALUES {
            level.update_neighbors_at(pos.relative(direction, 1), Block::REDSTONE_WIRE)?;
        }
    }
    Ok(())
}
fn update_neighboring_wires(level: &mut Level, pos: BlockPos) -> Result<(), LevelError> {
    for direction in HORIZONTAL {
        check_corner(level, pos.relative(direction, 1))?;
    }
    for direction in HORIZONTAL {
        let target = pos.relative(direction, 1);
        let target = if level.is_redstone_conductor(level.get_block_state(target)?, target)? {
            target.above(1)
        } else {
            target.below(1)
        };
        check_corner(level, target)?;
    }
    Ok(())
}
impl BlockBehavior for RedstoneWireBlock {
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &crate::block::UseContext,
        level: &mut Level,
    ) -> Result<crate::block::InteractionResult, LevelError> {
        use crate::{block::InteractionResult, entity::player::ServerPlayer};
        if !level.entity::<ServerPlayer>(context.player)?.may_build() {
            return Ok(InteractionResult::Pass);
        }
        if is_cross(state) || is_dot(state) {
            let candidate = if is_cross(state) {
                Block::REDSTONE_WIRE.default_state()
            } else {
                cross_state()
            }
            .with(p::POWER, state.get(p::POWER).unwrap())
            .unwrap();
            let new = connection_state(&LevelReaderRef::Server(level), candidate, pos)?;
            if new != state {
                level.set_block_and_update(pos, new)?;
                let orientation = super::initial_orientation(level, None, Some(Direction::Up));
                for direction in HORIZONTAL {
                    let target = pos.relative(direction, 1);
                    if side(state, direction).connected() != side(new, direction).connected()
                        && level.is_redstone_conductor(level.get_block_state(target)?, target)?
                    {
                        level.update_neighbors_at_except_from_facing(
                            target,
                            new.block(),
                            Some(direction.opposite()),
                            orientation.map(|o| o.with_front(direction)),
                        )?;
                    }
                }
                return Ok(InteractionResult::SUCCESS);
            }
        }
        Ok(InteractionResult::Pass)
    }

    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(i32::from(wire_power(state)))
    }
    fn should_redstone_wire_connect_to(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _direction: Option<Direction>,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if !level.wire_signals_enabled() || direction == Direction::Down {
            return Ok(0);
        }
        let power = wire_power(state);
        if power == 0 {
            return Ok(0);
        }
        Ok(
            if direction != Direction::Up
                && !side(connection_state(level, state, pos)?, direction.opposite()).connected()
            {
                0
            } else {
                i32::from(power)
            },
        )
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        self.signal(state, pos, direction, level)
    }
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = pos.below(1);
        survives_on(level, below, level.get_block_state(below)?)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        let reader = level.as_reader();
        if direction == Direction::Down {
            return Ok(if survives_on(&reader, neighbor_pos, neighbor)? {
                state
            } else {
                Block::AIR.default_state()
            });
        }
        if direction == Direction::Up {
            return connection_state(&reader, state, pos);
        }
        let can_up = !reader.is_redstone_conductor(reader.get_block_state(pos.above(1))?, pos)?;
        let connection = connecting_side(&reader, pos, direction, can_up)?;
        if connection.connected() == side(state, direction).connected() && !is_cross(state) {
            Ok(with_side(state, direction, connection))
        } else {
            connection_state(
                &reader,
                with_side(
                    cross_state().with(p::POWER, wire_power(state)).unwrap(),
                    direction,
                    connection,
                ),
                pos,
            )
        }
    }
    fn update_indirect_neighbor_shapes(
        &self,
        state: BlockState,
        pos: BlockPos,
        flags: UpdateFlags,
        limit: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        for direction in HORIZONTAL {
            let neighbor = pos.relative(direction, 1);
            if side(state, direction) != RedstoneSide::None
                && level.get_block_state(neighbor)?.block() != Block::REDSTONE_WIRE
            {
                for target in [neighbor.below(1), neighbor.above(1)] {
                    if level.get_block_state(target)?.block() == Block::REDSTONE_WIRE {
                        let relative = target.relative(direction.opposite(), 1);
                        level.neighbor_shape_changed(
                            direction.opposite(),
                            target,
                            relative,
                            level.get_block_state(relative)?,
                            flags,
                            limit,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() {
            update_power(level, pos, state, None, true)?;
            // Direction.Plane.VERTICAL is UP, DOWN.
            for direction in [Direction::Up, Direction::Down] {
                level.update_neighbors_at(pos.relative(direction, 1), Block::REDSTONE_WIRE)?;
            }
            update_neighboring_wires(level, pos)?;
        }
        Ok(())
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !moved {
            for direction in Direction::VALUES {
                level.update_neighbors_at(pos.relative(direction, 1), Block::REDSTONE_WIRE)?;
            }
            update_power(level, pos, state, None, false)?;
            update_neighboring_wires(level, pos)?;
        }
        Ok(())
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        source: Block,
        orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if source != Block::REDSTONE_WIRE || !level.rules().redstone_experiments {
            if self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
                update_power(level, pos, state, orientation, false)?;
            } else {
                crate::block::drops::drop_resources_simple(state, pos, level)?;
                level.remove_block(pos, false)?;
            }
        }
        Ok(())
    }
}

/// Port of ExperimentalRedstoneWireEvaluator. Maps retain first insertion order
/// and first orientation even if the same wire is revisited by another branch.
#[derive(Default)]
struct ExperimentalRedstoneWireEvaluator {
    turn_off: VecDeque<BlockPos>,
    turn_on: VecDeque<BlockPos>,
    updated: Object2IntLinkedOpenHashMap<BlockPos>,
}
impl ExperimentalRedstoneWireEvaluator {
    fn set_power(&mut self, pos: BlockPos, power: u8, orientation: Orientation) {
        let orientation = self
            .updated
            .get(&pos)
            .map_or(orientation, |packed| unpack_orientation(*packed));
        self.updated.insert(
            pos,
            (i32::from(orientation.index()) << 4) | i32::from(power),
        );
    }
    fn power(&self, pos: BlockPos, state: BlockState) -> u8 {
        self.updated
            .get(&pos)
            .map_or_else(|| wire_power(state), |p| (p & 15) as u8)
    }
    fn enqueue(
        &mut self,
        level: &Level,
        pos: BlockPos,
        from_power: u8,
        orientation: Orientation,
        allow_off: bool,
    ) -> Result<(), LevelError> {
        let state = level.get_block_state(pos)?;
        if state.block() == Block::REDSTONE_WIRE {
            let power = self.power(pos, state);
            if i32::from(power) < i32::from(from_power) - 1 && !self.turn_on.contains(&pos) {
                self.turn_on.push_back(pos);
                self.set_power(pos, power, orientation);
            }
            if allow_off && power > from_power && !self.turn_off.contains(&pos) {
                self.turn_off.push_back(pos);
                self.set_power(pos, power, orientation);
            }
        }
        Ok(())
    }
    fn propagate(
        &mut self,
        level: &Level,
        pos: BlockPos,
        power: u8,
        orientation: Orientation,
        allow_off: bool,
    ) -> Result<(), LevelError> {
        for direction in orientation_horizontal(orientation) {
            self.enqueue(
                level,
                pos.relative(direction, 1),
                power,
                orientation.with_front(direction),
                allow_off,
            )?;
        }
        for vertical in [orientation.up().opposite(), orientation.up()] {
            let offset = pos.relative(vertical, 1);
            let solid = level.is_redstone_conductor(level.get_block_state(offset)?, offset)?;
            for horizontal in orientation_horizontal(orientation) {
                let neighbor = pos.relative(horizontal, 1);
                if (vertical == Direction::Up && !solid)
                    || (vertical == Direction::Down
                        && !level
                            .is_redstone_conductor(level.get_block_state(neighbor)?, neighbor)?)
                {
                    self.enqueue(
                        level,
                        offset.relative(horizontal, 1),
                        power,
                        orientation.with_front(horizontal),
                        allow_off,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn update(
        mut self,
        level: &mut Level,
        pos: BlockPos,
        orientation: Option<Orientation>,
        shape_initial: bool,
    ) -> Result<(), LevelError> {
        let orientation = orientation
            .unwrap_or_else(|| {
                Orientation::from_index(level.random().next_int_bound(48) as u8).unwrap()
            })
            .with_up(Direction::Up)
            .with_side_bias(SideBias::Left);
        let state = level.get_block_state(pos)?;
        if state.block() == Block::REDSTONE_WIRE {
            self.set_power(pos, wire_power(state), orientation);
            self.turn_off.push_back(pos);
        } else {
            self.propagate(level, pos, 0, orientation, true)?;
        }
        while let Some(pos) = self.turn_off.pop_front() {
            let packed = *self.updated.get(&pos).unwrap();
            let old = (packed & 15) as u8;
            let orientation = unpack_orientation(packed);
            let external = block_signal(level, pos)?;
            let new = u8::try_from(
                external.max(incoming_wire_signal(level, pos, |p, s| self.power(p, s))?),
            )
            .expect("wire power range");
            let set = if new < old {
                if external > 0 && !self.turn_on.contains(&pos) {
                    self.turn_on.push_back(pos);
                }
                0
            } else {
                new
            };
            if set != old {
                self.set_power(pos, set, orientation);
            }
            self.propagate(level, pos, set, orientation, old > new)?;
        }
        while let Some(pos) = self.turn_on.pop_front() {
            let packed = *self.updated.get(&pos).unwrap();
            let old = (packed & 15) as u8;
            let orientation = unpack_orientation(packed);
            let external = block_signal(level, pos)?;
            let new = u8::try_from(
                external.max(incoming_wire_signal(level, pos, |p, s| self.power(p, s))?),
            )
            .expect("wire power range");
            assert!(new >= old, "Turning off wire while trying to turn it on");
            if new > old {
                self.set_power(pos, new, orientation);
            }
            self.propagate(level, pos, new, orientation, false)?;
        }
        let changes: Vec<_> = self.updated.iter().map(|(p, v)| (*p, *v)).collect();
        let mut changed = Vec::new();
        for (i, (pos, packed)) in changes.into_iter().enumerate() {
            let state = level.get_block_state(pos)?;
            let power = (packed & 15) as u8;
            if state.block() == Block::REDSTONE_WIRE && wire_power(state) != power {
                let flags = if !shape_initial || i != 0 {
                    UpdateFlags::CLIENTS | UpdateFlags::SKIP_SHAPE_UPDATE_ON_WIRE
                } else {
                    UpdateFlags::CLIENTS
                };
                level.set_block(
                    pos,
                    state.with(p::POWER, power).unwrap(),
                    flags,
                    UPDATE_LIMIT,
                )?;
                changed.push((pos, unpack_orientation(packed)));
            }
        }
        for (pos, orientation) in changed {
            let state = level.get_block_state(pos)?;
            for direction in orientation_directions(orientation) {
                if direction == Direction::Down
                    || (direction.axis() != ferraloom_util::Axis::Y
                        && side(state, direction).connected())
                {
                    let neighbor = pos.relative(direction, 1);
                    let state = level.get_block_state(neighbor)?;
                    let oriented = with_front_preserve_up(orientation, direction);
                    level.neighbor_changed_with_state(
                        state,
                        neighbor,
                        Block::REDSTONE_WIRE,
                        Some(oriented),
                        false,
                    )?;
                    if level.is_redstone_conductor(state, neighbor)? {
                        for next in orientation_directions(oriented) {
                            if next != direction.opposite() {
                                level.neighbor_changed(
                                    neighbor.relative(next, 1),
                                    Block::REDSTONE_WIRE,
                                    Some(with_front_preserve_up(oriented, next)),
                                )?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn unpack_orientation(packed: i32) -> Orientation {
    Orientation::from_index((packed >> 4) as u8).unwrap()
}
fn with_front_preserve_up(orientation: Orientation, front: Direction) -> Orientation {
    if front.axis() == orientation.up().axis() {
        orientation
    } else {
        orientation.with_front(front)
    }
}
pub(super) fn orientation_directions(orientation: Orientation) -> [Direction; 6] {
    let front = orientation.front();
    let up = orientation.up();
    let a = front.step();
    let b = up.step();
    let cross = (
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    );
    let right = Direction::VALUES
        .into_iter()
        .find(|d| {
            let s = d.step();
            (s.x, s.y, s.z) == cross
        })
        .unwrap();
    let side = if orientation.side_bias() == SideBias::Right {
        right
    } else {
        right.opposite()
    };
    [
        front.opposite(),
        front,
        side,
        side.opposite(),
        up.opposite(),
        up,
    ]
}
fn orientation_horizontal(orientation: Orientation) -> [Direction; 4] {
    let d = orientation_directions(orientation);
    [d[0], d[1], d[2], d[3]]
}
