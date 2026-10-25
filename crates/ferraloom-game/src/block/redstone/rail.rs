//! Port of BaseRailBlock, RailBlock, PoweredRailBlock and RailState. Connection
//! lists retain vanilla's insertion order through synchronous neighbor callbacks.
use super::{diode, state::HORIZONTAL};
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    level::{Level, LevelError, redstone::Orientation},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction, VoxelShape, shapes};

pub struct RailBlock;
pub static RAIL: RailBlock = RailBlock;

fn shape(state: BlockState) -> p::Shape {
    state.get(p::SHAPE).unwrap()
}
fn straight(state: BlockState) -> bool {
    state.block().class() != BlockClass::RailBlock
}
fn slope(shape: p::Shape) -> Option<Direction> {
    match shape {
        p::Shape::AscendingEast => Some(Direction::East),
        p::Shape::AscendingWest => Some(Direction::West),
        p::Shape::AscendingNorth => Some(Direction::North),
        p::Shape::AscendingSouth => Some(Direction::South),
        _ => None,
    }
}
fn is_rail(level: &Level, pos: BlockPos) -> Result<bool, LevelError> {
    let state = level.get_block_state(pos)?;
    Ok(level
        .block_tags()?
        .contains_key(BlockTags::RAILS, state.block())
        && state.block().is_instance_of(BlockClass::BaseRailBlock))
}

/// Port of RailState. Transient state contains no canonical world borrow.
pub struct RailState {
    pos: BlockPos,
    state: BlockState,
    connections: Vec<BlockPos>,
}
impl RailState {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        assert!(state.block().is_instance_of(BlockClass::BaseRailBlock));
        let mut rail = Self {
            pos,
            state,
            connections: Vec::with_capacity(2),
        };
        rail.update_connections(shape(state));
        rail
    }
    pub fn state(&self) -> BlockState {
        self.state
    }
    pub fn connections(&self) -> &[BlockPos] {
        &self.connections
    }
    fn update_connections(&mut self, shape: p::Shape) {
        let pos = self.pos;
        use p::Shape as S;
        let connections = match shape {
            S::NorthSouth => [pos.north(1), pos.south(1)],
            S::EastWest => [pos.west(1), pos.east(1)],
            S::AscendingEast => [pos.west(1), pos.east(1).above(1)],
            S::AscendingWest => [pos.west(1).above(1), pos.east(1)],
            S::AscendingNorth => [pos.north(1).above(1), pos.south(1)],
            S::AscendingSouth => [pos.north(1), pos.south(1).above(1)],
            S::SouthEast => [pos.east(1), pos.south(1)],
            S::SouthWest => [pos.west(1), pos.south(1)],
            S::NorthWest => [pos.west(1), pos.north(1)],
            S::NorthEast => [pos.east(1), pos.north(1)],
            _ => unreachable!("rail shape"),
        };
        self.connections.clear();
        self.connections.extend(connections);
    }
    fn get_rail(level: &Level, pos: BlockPos) -> Result<Option<Self>, LevelError> {
        for candidate in [pos, pos.above(1), pos.below(1)] {
            if is_rail(level, candidate)? {
                return Ok(Some(Self::new(
                    candidate,
                    level.get_block_state(candidate)?,
                )));
            }
        }
        Ok(None)
    }
    fn has_connection(&self, pos: BlockPos) -> bool {
        self.connections
            .iter()
            .any(|p| p.x == pos.x && p.z == pos.z)
    }
    fn remove_soft_connections(&mut self, level: &Level) -> Result<(), LevelError> {
        let mut index = 0;
        while index < self.connections.len() {
            if let Some(rail) = Self::get_rail(level, self.connections[index])?
                && rail.has_connection(self.pos)
            {
                self.connections[index] = rail.pos;
                index += 1;
            } else {
                self.connections.remove(index);
            }
        }
        Ok(())
    }
    fn can_connect_to(&self, rail: &Self) -> bool {
        self.has_connection(rail.pos) || self.connections.len() != 2
    }
    fn has_neighbor_rail(&self, level: &Level, pos: BlockPos) -> Result<bool, LevelError> {
        let Some(mut neighbor) = Self::get_rail(level, pos)? else {
            return Ok(false);
        };
        neighbor.remove_soft_connections(level)?;
        Ok(neighbor.can_connect_to(self))
    }
    fn ascending(
        &self,
        level: &Level,
        shape: Option<p::Shape>,
    ) -> Result<Option<p::Shape>, LevelError> {
        use p::Shape as S;
        let mut result = shape;
        if result == Some(S::NorthSouth) {
            if is_rail(level, self.pos.north(1).above(1))? {
                result = Some(S::AscendingNorth);
            }
            if is_rail(level, self.pos.south(1).above(1))? {
                result = Some(S::AscendingSouth);
            }
        }
        if result == Some(S::EastWest) {
            if is_rail(level, self.pos.east(1).above(1))? {
                result = Some(S::AscendingEast);
            }
            if is_rail(level, self.pos.west(1).above(1))? {
                result = Some(S::AscendingWest);
            }
        }
        Ok(result)
    }
    fn corner(&self, n: bool, s: bool, w: bool, e: bool) -> Option<p::Shape> {
        use p::Shape as S;
        if straight(self.state) {
            return None;
        }
        match (n, s, w, e) {
            (false, true, false, true) => Some(S::SouthEast),
            (false, true, true, false) => Some(S::SouthWest),
            (true, false, true, false) => Some(S::NorthWest),
            (true, false, false, true) => Some(S::NorthEast),
            _ => None,
        }
    }
    fn connect_to(&mut self, rail: &Self, level: &mut Level) -> Result<(), LevelError> {
        self.connections.push(rail.pos);
        let n = self.has_connection(self.pos.north(1));
        let s = self.has_connection(self.pos.south(1));
        let w = self.has_connection(self.pos.west(1));
        let e = self.has_connection(self.pos.east(1));
        let mut selected = (n || s).then_some(p::Shape::NorthSouth);
        if w || e {
            selected = Some(p::Shape::EastWest);
        }
        if let Some(corner) = self.corner(n, s, w, e) {
            selected = Some(corner);
        }
        let selected = self
            .ascending(level, selected)?
            .unwrap_or(p::Shape::NorthSouth);
        self.state = self.state.with(p::SHAPE, selected).unwrap();
        level.set_block_and_update(self.pos, self.state)?;
        Ok(())
    }
    pub fn count_potential_connections(&self, level: &Level) -> Result<u8, LevelError> {
        let mut count = 0;
        for direction in HORIZONTAL {
            if Self::get_rail(level, self.pos.relative(direction, 1))?.is_some() {
                count += 1;
            }
        }
        Ok(count)
    }
    /// Port of RailState#place, including powered corner preference and the
    /// captured connection list used after setBlock's reentrant callbacks.
    pub fn place(
        &mut self,
        level: &mut Level,
        has_signal: bool,
        first: bool,
        default: p::Shape,
    ) -> Result<(), LevelError> {
        use p::Shape as S;
        let n = self.has_neighbor_rail(level, self.pos.north(1))?;
        let s = self.has_neighbor_rail(level, self.pos.south(1))?;
        let w = self.has_neighbor_rail(level, self.pos.west(1))?;
        let e = self.has_neighbor_rail(level, self.pos.east(1))?;
        let ns = n || s;
        let we = w || e;
        let mut selected = match (ns, we) {
            (true, false) => Some(S::NorthSouth),
            (false, true) => Some(S::EastWest),
            _ => None,
        };
        if let Some(corner) = self.corner(n, s, w, e) {
            selected = Some(corner);
        }
        if selected.is_none() {
            selected = match (ns, we) {
                (true, true) => Some(default),
                (true, false) => Some(S::NorthSouth),
                (false, true) => Some(S::EastWest),
                (false, false) => None,
            };
            if !straight(self.state) {
                let candidates = if has_signal {
                    [
                        (s && e, S::SouthEast),
                        (s && w, S::SouthWest),
                        (n && e, S::NorthEast),
                        (n && w, S::NorthWest),
                    ]
                } else {
                    [
                        (n && w, S::NorthWest),
                        (n && e, S::NorthEast),
                        (s && w, S::SouthWest),
                        (s && e, S::SouthEast),
                    ]
                };
                for (connected, corner) in candidates {
                    if connected {
                        selected = Some(corner);
                    }
                }
            }
        }
        let selected = self.ascending(level, selected)?.unwrap_or(default);
        self.update_connections(selected);
        self.state = self.state.with(p::SHAPE, selected).unwrap();
        if first || level.get_block_state(self.pos)? != self.state {
            level.set_block_and_update(self.pos, self.state)?;
            for &connection in &self.connections {
                if let Some(mut neighbor) = Self::get_rail(level, connection)? {
                    neighbor.remove_soft_connections(level)?;
                    if neighbor.can_connect_to(self) {
                        neighbor.connect_to(self, level)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Port of PoweredRailBlock#findPoweredRailSignal. Both traversal directions
/// count the origin toward the eight-rail search limit.
pub fn find_powered_rail_signal(
    level: &Level,
    pos: BlockPos,
    state: BlockState,
    forward: bool,
    depth: u8,
) -> Result<bool, LevelError> {
    use p::Shape as S;
    if depth >= 8 {
        return Ok(false);
    }
    let (direction, upward, axis) = match shape(state) {
        S::NorthSouth => (
            if forward {
                Direction::South
            } else {
                Direction::North
            },
            false,
            S::NorthSouth,
        ),
        S::EastWest => (
            if forward {
                Direction::West
            } else {
                Direction::East
            },
            false,
            S::EastWest,
        ),
        S::AscendingEast => (
            if forward {
                Direction::West
            } else {
                Direction::East
            },
            !forward,
            S::EastWest,
        ),
        S::AscendingWest => (
            if forward {
                Direction::West
            } else {
                Direction::East
            },
            forward,
            S::EastWest,
        ),
        S::AscendingNorth => (
            if forward {
                Direction::South
            } else {
                Direction::North
            },
            !forward,
            S::NorthSouth,
        ),
        S::AscendingSouth => (
            if forward {
                Direction::South
            } else {
                Direction::North
            },
            forward,
            S::NorthSouth,
        ),
        _ => unreachable!("straight rail shape"),
    };
    let next = pos.relative(direction, 1).above(i32::from(upward));
    for candidate in [Some(next), (!upward).then_some(next.below(1))]
        .into_iter()
        .flatten()
    {
        let neighbor = level.get_block_state(candidate)?;
        if neighbor.block() != state.block() || !neighbor.get(p::POWERED).unwrap() {
            continue;
        }
        let other_shape = shape(neighbor);
        if (axis == S::EastWest
            && matches!(
                other_shape,
                S::NorthSouth | S::AscendingNorth | S::AscendingSouth
            ))
            || (axis == S::NorthSouth
                && matches!(
                    other_shape,
                    S::EastWest | S::AscendingEast | S::AscendingWest
                ))
        {
            continue;
        }
        if level.has_neighbor_signal(candidate)?
            || find_powered_rail_signal(level, candidate, neighbor, forward, depth + 1)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn update_dir(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    first: bool,
) -> Result<BlockState, LevelError> {
    let powered = level.has_neighbor_signal(pos)?;
    let mut rail = RailState::new(pos, state);
    rail.place(level, powered, first, shape(state))?;
    Ok(rail.state())
}
impl BlockBehavior for RailBlock {
    fn shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(shapes::box_shape(
            0.0,
            0.0,
            0.0,
            1.0,
            if slope(shape(state)).is_some() {
                0.5
            } else {
                0.125
            },
            1.0,
        ))
    }
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = pos.below(1);
        diode::survives_on(level, below, level.get_block_state(below)?)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.get(p::WATERLOGGED).unwrap() {
            level.schedule_fluid_tick(pos, ferraloom_data::registries::fluid::Id::WATER, 5)?;
        }
        Ok(state)
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() {
            let updated = update_dir(level, pos, state, true)?;
            if straight(updated) {
                level.neighbor_changed_with_state(updated, pos, state.block(), None, moved)?;
            }
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
            if slope(shape(state)).is_some() {
                level.update_neighbors_at(pos.above(1), state.block())?;
            }
            if straight(state) {
                level.update_neighbors_at(pos, state.block())?;
                level.update_neighbors_at(pos.below(1), state.block())?;
            }
        }
        Ok(())
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        source: Block,
        _orientation: Option<Orientation>,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if level.get_block_state(pos)?.block() != state.block() {
            return Ok(());
        }
        let reader = LevelReaderRef::Server(level);
        let unsupported = !self.can_survive(state, pos, &reader)?
            || if let Some(direction) = slope(shape(state)) {
                let support = pos.relative(direction, 1);
                !diode::survives_on(&reader, support, reader.get_block_state(support)?)?
            } else {
                false
            };
        if unsupported {
            crate::block::drops::drop_resources_simple(state, pos, level)?;
            level.remove_block(pos, moved)?;
        } else if state.block().class() == BlockClass::RailBlock {
            if LevelReaderRef::Server(level).is_signal_source(source.default_state())?
                && RailState::new(pos, state).count_potential_connections(level)? == 3
            {
                update_dir(level, pos, state, false)?;
            }
        } else if state.block().class() == BlockClass::PoweredRailBlock {
            let powered = level.has_neighbor_signal(pos)?
                || find_powered_rail_signal(level, pos, state, true, 0)?
                || find_powered_rail_signal(level, pos, state, false, 0)?;
            if powered != state.get(p::POWERED).unwrap() {
                level.set_block_and_update(pos, state.with(p::POWERED, powered).unwrap())?;
                level.update_neighbors_at(pos.below(1), state.block())?;
                if slope(shape(state)).is_some() {
                    level.update_neighbors_at(pos.above(1), state.block())?;
                }
            }
        }
        Ok(())
    }
}

pub struct DetectorRailBlock;
pub static DETECTOR: DetectorRailBlock = DetectorRailBlock;
/// No currently canonical entity subclass extends AbstractMinecart. Keeping this
/// match exhaustive makes a new vehicle require its native detector integration.
fn is_minecart(entity: &crate::entity::EntityState) -> bool {
    use crate::entity::EntityState as E;
    match entity {
        E::Cow(_)
        | E::Player(_)
        | E::PrimedTnt(_)
        | E::Item(_)
        | E::FallingBlock(_)
        | E::ExperienceOrb(_) => false,
    }
}
fn interacting_minecarts(pos: BlockPos, level: &Level) -> Vec<crate::entity::EntityRef> {
    level.entities_in(
        ferraloom_util::Aabb::new(
            f64::from(pos.x) + 0.2,
            f64::from(pos.y),
            f64::from(pos.z) + 0.2,
            f64::from(pos.x + 1) - 0.2,
            f64::from(pos.y + 1) - 0.2,
            f64::from(pos.z + 1) - 0.2,
        ),
        |_, entity| is_minecart(entity),
    )
}
/// Port of DetectorRailBlock#checkPressed. Native setBlocksDirty is inherited
/// Level's empty server method; connected rails receive their own block as source.
fn check_pressed(state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    if !RAIL.can_survive(state, pos, &LevelReaderRef::Server(level))?
        || level.get_block_state(pos)?.block() != state.block()
    {
        return Ok(());
    }
    let minecarts = interacting_minecarts(pos, level);
    // Vehicle projection must be provided with the first actual minecart port.
    if let Some(entity) = minecarts.first() {
        return Err(LevelError::UnsupportedEntity(
            level.entity_state(*entity)?.entity_type(),
        ));
    }
    let pressed = !minecarts.is_empty();
    if pressed != state.get(p::POWERED).unwrap() {
        let changed = state.with(p::POWERED, pressed).unwrap();
        level.set_block_and_update(pos, changed)?;
        for &connection in RailState::new(pos, changed).connections() {
            let neighbor = level.get_block_state(connection)?;
            level.neighbor_changed_with_state(
                neighbor,
                connection,
                neighbor.block(),
                None,
                false,
            )?;
        }
        level.update_neighbors_at(pos, state.block())?;
        level.update_neighbors_at(pos.below(1), state.block())?;
    }
    if pressed {
        level.schedule_block_tick_after(pos, state.block(), 20)?;
    }
    level.update_neighbour_for_output_signal(pos, state.block())
}
impl BlockBehavior for DetectorRailBlock {
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() {
            let updated = update_dir(level, pos, state, true)?;
            level.neighbor_changed_with_state(updated, pos, state.block(), None, moved)?;
            check_pressed(updated, pos, level)?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if state.get(p::POWERED).unwrap() {
            check_pressed(state, pos, level)?;
        }
        Ok(())
    }
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        _entity: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !state.get(p::POWERED).unwrap() {
            check_pressed(state, pos, level)?;
        }
        Ok(())
    }
    fn analog_output_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        level: &mut Level,
    ) -> Result<i32, LevelError> {
        if state.get(p::POWERED).unwrap()
            && let Some(entity) = interacting_minecarts(pos, level).first()
        {
            return Err(LevelError::UnsupportedEntity(
                level.entity_state(*entity)?.entity_type(),
            ));
        }
        Ok(0)
    }
}
