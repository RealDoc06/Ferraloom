//! Port of PistonBaseBlock#isPushable and PistonStructureResolver. The ordered
//! push/destroy lists are used as-is by movement, never coordinate-sorted.
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, PushReaction, properties};
use ferraloom_util::{BlockPos, Direction};

/// Port of PistonBaseBlock#isPushable, including the 26.3 PUSH reaction's
/// connection-direction check and its early return before block-entity checks.
pub fn is_pushable(
    state: BlockState,
    level: &Level,
    pos: BlockPos,
    direction: Direction,
    allow_destroyable: bool,
    connection_direction: Direction,
) -> bool {
    if pos.y < level.min_build_height()
        || pos.y > level.max_build_height()
        || !level.world_border().is_within_block(pos)
    {
        return false;
    }
    if state.is_air() {
        return true;
    }
    if (direction == Direction::Down && pos.y == level.min_build_height())
        || (direction == Direction::Up && pos.y == level.max_build_height())
    {
        return false;
    }
    if matches!(state.block(), Block::PISTON | Block::STICKY_PISTON) {
        if state.get(properties::EXTENDED).unwrap() {
            return false;
        }
    } else {
        if state.data().destroy_time == -1.0 {
            return false;
        }
        match state.data().push_reaction {
            PushReaction::Immoveable => return false,
            PushReaction::Popped => return allow_destroyable,
            PushReaction::Push => return direction == connection_direction,
            PushReaction::PushPull | PushReaction::IgnoreEntity => {}
        }
    }
    !state.has_block_entity()
}

pub fn is_sticky(state: BlockState) -> bool {
    matches!(state.block(), Block::SLIME_BLOCK | Block::HONEY_BLOCK)
}
/// Port of PistonStructureResolver#canStickToEachOther.
pub fn can_stick_to_each_other(first: BlockState, second: BlockState) -> bool {
    !matches!(
        (first.block(), second.block()),
        (Block::SLIME_BLOCK, Block::HONEY_BLOCK) | (Block::HONEY_BLOCK, Block::SLIME_BLOCK)
    ) && (is_sticky(first) || is_sticky(second))
}

/// Port of PistonStructureResolver. Reusing a resolver clears both lists before
/// every attempt, including attempts that fail after collecting some branches.
pub struct PistonStructureResolver {
    piston_pos: BlockPos,
    piston_direction: Direction,
    extending: bool,
    start_pos: BlockPos,
    push_direction: Direction,
    to_push: Vec<BlockPos>,
    to_destroy: Vec<BlockPos>,
}
impl PistonStructureResolver {
    pub const MAX_PUSH_DEPTH: usize = 12;
    pub fn new(piston_pos: BlockPos, direction: Direction, extending: bool) -> Self {
        Self {
            piston_pos,
            piston_direction: direction,
            extending,
            start_pos: piston_pos.relative(direction, if extending { 1 } else { 2 }),
            push_direction: if extending {
                direction
            } else {
                direction.opposite()
            },
            to_push: Vec::new(),
            to_destroy: Vec::new(),
        }
    }
    pub fn push_direction(&self) -> Direction {
        self.push_direction
    }
    pub fn to_push(&self) -> &[BlockPos] {
        &self.to_push
    }
    pub fn to_destroy(&self) -> &[BlockPos] {
        &self.to_destroy
    }
    pub fn resolve(&mut self, level: &Level) -> Result<bool, LevelError> {
        self.to_push.clear();
        self.to_destroy.clear();
        let next = level.get_block_state(self.start_pos)?;
        if !is_pushable(
            next,
            level,
            self.start_pos,
            self.push_direction,
            false,
            self.piston_direction,
        ) {
            if self.extending && next.data().push_reaction == PushReaction::Popped {
                self.to_destroy.push(self.start_pos);
                return Ok(true);
            }
            return Ok(false);
        }
        if !self.add_block_line(level, self.start_pos, self.push_direction)? {
            return Ok(false);
        }
        let mut i = 0;
        while i < self.to_push.len() {
            let pos = self.to_push[i];
            if is_sticky(level.get_block_state(pos)?) && !self.add_branching_blocks(level, pos)? {
                return Ok(false);
            }
            i += 1;
        }
        Ok(true)
    }
    fn add_block_line(
        &mut self,
        level: &Level,
        start: BlockPos,
        direction: Direction,
    ) -> Result<bool, LevelError> {
        let mut next = level.get_block_state(start)?;
        if next.is_air()
            || !is_pushable(next, level, start, self.push_direction, false, direction)
            || start == self.piston_pos
            || self.to_push.contains(&start)
        {
            return Ok(true);
        }
        let mut count = 1;
        if count + self.to_push.len() > Self::MAX_PUSH_DEPTH {
            return Ok(false);
        }
        while is_sticky(next) {
            let pos = start.relative(self.push_direction.opposite(), count as i32);
            let previous = next;
            next = level.get_block_state(pos)?;
            if next.is_air()
                || !can_stick_to_each_other(previous, next)
                || !is_pushable(
                    next,
                    level,
                    pos,
                    self.push_direction,
                    false,
                    self.push_direction.opposite(),
                )
                || pos == self.piston_pos
            {
                break;
            }
            count += 1;
            if count + self.to_push.len() > Self::MAX_PUSH_DEPTH {
                return Ok(false);
            }
        }
        let mut added = count;
        for i in (0..count).rev() {
            self.to_push
                .push(start.relative(self.push_direction.opposite(), i as i32));
        }
        for i in 1.. {
            let pos = start.relative(self.push_direction, i);
            if let Some(collision) = self.to_push.iter().position(|p| *p == pos) {
                self.reorder_list_at_collision(added, collision);
                for j in 0..=collision + added {
                    let branch = self.to_push[j];
                    if is_sticky(level.get_block_state(branch)?)
                        && !self.add_branching_blocks(level, branch)?
                    {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            next = level.get_block_state(pos)?;
            if next.is_air() {
                return Ok(true);
            }
            if !is_pushable(
                next,
                level,
                pos,
                self.push_direction,
                true,
                self.push_direction,
            ) || pos == self.piston_pos
            {
                return Ok(false);
            }
            if next.data().push_reaction == PushReaction::Popped {
                self.to_destroy.push(pos);
                return Ok(true);
            }
            if self.to_push.len() >= Self::MAX_PUSH_DEPTH {
                return Ok(false);
            }
            self.to_push.push(pos);
            added += 1;
        }
        unreachable!()
    }
    fn reorder_list_at_collision(&mut self, added: usize, collision: usize) {
        // The tail just added moves in front of the collision-to-tail segment.
        let tail = self.to_push.len() - added;
        self.to_push[collision..].rotate_left(tail - collision);
    }
    fn add_branching_blocks(&mut self, level: &Level, from: BlockPos) -> Result<bool, LevelError> {
        let state = level.get_block_state(from)?;
        for direction in Direction::VALUES {
            if direction.axis() != self.push_direction.axis() {
                let neighbor = from.relative(direction, 1);
                if can_stick_to_each_other(level.get_block_state(neighbor)?, state)
                    && !self.add_block_line(level, neighbor, direction)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

use super::{
    initial_orientation,
    moving_piston::{self, PistonMovingBlockEntity},
    state::{facing, with_facing},
};
use crate::{
    block::{
        BlockBehavior, BlockMethod, UPDATE_LIMIT, UpdateFlags,
        entity::{BlockEntityData, BlockEntityDataView, BlockEntityState, BlockEntityType},
    },
    level::{events::BlockEvent, redstone::Orientation},
};
use ferraloom_data::registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{Vec3, collections::JavaHashMap, random::RandomSource};

pub struct PistonBaseBlock;
pub static PISTON: PistonBaseBlock = PistonBaseBlock;
/// Port of PistonBaseBlock#getNeighborSignal. Quasi-connectivity only changes
/// the power query: it does not add a new notification source above the piston.
pub fn neighbor_signal(level: &Level, pos: BlockPos, push: Direction) -> Result<bool, LevelError> {
    for direction in Direction::VALUES {
        if direction != push && level.has_signal(pos.relative(direction, 1), direction)? {
            return Ok(true);
        }
    }
    if level.has_signal(pos, Direction::Down)? {
        return Ok(true);
    }
    let above = pos.above(1);
    for direction in Direction::VALUES {
        if direction != Direction::Down
            && level.has_signal(above.relative(direction, 1), direction)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn moving_entity_at(
    level: &Level,
    pos: BlockPos,
) -> Result<Option<crate::block::entity::BlockEntityRef>, LevelError> {
    Ok(match level.block_entity_ref(pos)? {
        Some(handle)
            if PistonMovingBlockEntity::view(level.block_entity_state(handle)?).is_some() =>
        {
            Some(handle)
        }
        _ => None,
    })
}
fn check_if_extend(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let direction = facing(state);
    let extend = neighbor_signal(level, pos, direction)?;
    if extend && !state.get(properties::EXTENDED).unwrap() {
        if PistonStructureResolver::new(pos, direction, true).resolve(level)? {
            level.block_event(BlockEvent {
                pos,
                block: state.block(),
                param_a: 0,
                param_b: direction as i32,
            });
        }
    } else if !extend && state.get(properties::EXTENDED).unwrap() {
        let pushed = pos.relative(direction, 2);
        let pushed_state = level.get_block_state(pushed)?;
        let mut event = 1;
        if pushed_state.block() == Block::MOVING_PISTON
            && facing(pushed_state) == direction
            && let Some(handle) = moving_entity_at(level, pushed)?
        {
            let e = level.block_entity::<PistonMovingBlockEntity>(handle)?;
            if e.extending
                && (e.get_progress(0.0) < 0.5
                    || level.game_time() == e.last_ticked
                    || level.is_handling_tick())
            {
                event = 2;
            }
        }
        level.block_event(BlockEvent {
            pos,
            block: state.block(),
            param_a: event,
            param_b: direction as i32,
        });
    }
    Ok(())
}
fn set_moving_entity(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    moved: BlockState,
    direction: Direction,
    extending: bool,
    source: bool,
) -> Result<(), LevelError> {
    let data = BlockEntityData::new(BlockEntityType::PISTON, pos, state)?;
    level.set_block_entity(BlockEntityState::MovingPiston(Box::new(
        PistonMovingBlockEntity::moving(data, moved, direction, extending, source),
    )))
}
fn indirect_shapes(level: &mut Level, state: BlockState, pos: BlockPos) -> Result<(), LevelError> {
    level.optional_block_call(
        state.block(),
        BlockMethod::UpdateIndirectNeighborShapes,
        |behavior, level| {
            behavior.update_indirect_neighbor_shapes(
                state,
                pos,
                UpdateFlags::CLIENTS,
                UPDATE_LIMIT,
                level,
            )
        },
    )?;
    Ok(())
}
/// Port of PistonBaseBlock#moveBlocks. Hash-map deletion and final traversal use
/// the JDK map; push/destroy callbacks retain reverse-list ordering.
fn move_blocks(
    level: &mut Level,
    pos: BlockPos,
    direction: Direction,
    extending: bool,
    sticky: bool,
) -> Result<bool, LevelError> {
    let arm = pos.relative(direction, 1);
    if !extending && level.get_block_state(arm)?.block() == Block::PISTON_HEAD {
        level.set_block(
            arm,
            Block::AIR.default_state(),
            UpdateFlags(276),
            UPDATE_LIMIT,
        )?;
    }
    let mut resolver = PistonStructureResolver::new(pos, direction, extending);
    if !resolver.resolve(level)? {
        return Ok(false);
    }
    let mut delete = JavaHashMap::<BlockPos, BlockState>::new();
    let mut moved_states = Vec::with_capacity(resolver.to_push.len());
    for &pos in &resolver.to_push {
        let state = level.get_block_state(pos)?;
        moved_states.push(state);
        delete.insert(pos, state);
    }
    let mut to_update = Vec::with_capacity(resolver.to_push.len() + resolver.to_destroy.len());
    let push = if extending {
        direction
    } else {
        direction.opposite()
    };
    for &pos in resolver.to_destroy.iter().rev() {
        let state = level.get_block_state(pos)?;
        let block_entity = if state.has_block_entity() {
            level.get_or_promote_block_entity(pos)?
        } else {
            None
        };
        crate::block::drops::drop_resources(state, pos, block_entity, level)?;
        level.set_block(
            pos,
            Block::AIR.default_state(),
            UpdateFlags(18),
            UPDATE_LIMIT,
        )?;
        level.game_event(
            GameEvent::BLOCK_DESTROY,
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            None,
            Some(state),
        );
        to_update.push(state);
    }
    for (i, &old_pos) in resolver.to_push.iter().enumerate().rev() {
        let old = level.get_block_state(old_pos)?;
        let destination = old_pos.relative(push, 1);
        delete.remove(&destination);
        let actual = with_facing(Block::MOVING_PISTON.default_state(), direction);
        level.set_block(destination, actual, UpdateFlags(324), UPDATE_LIMIT)?;
        set_moving_entity(
            level,
            destination,
            actual,
            moved_states[i],
            direction,
            extending,
            false,
        )?;
        to_update.push(old);
    }
    if extending {
        let kind = if sticky {
            properties::Type::Sticky
        } else {
            properties::Type::Normal
        };
        let head = with_facing(
            Block::PISTON_HEAD
                .default_state()
                .with(properties::TYPE_, kind)
                .unwrap(),
            direction,
        );
        let actual = with_facing(
            Block::MOVING_PISTON
                .default_state()
                .with(properties::TYPE_, kind)
                .unwrap(),
            direction,
        );
        delete.remove(&arm);
        level.set_block(arm, actual, UpdateFlags(324), UPDATE_LIMIT)?;
        set_moving_entity(level, arm, actual, head, direction, true, true)?;
    }
    for &pos in delete.keys() {
        level.set_block(
            pos,
            Block::AIR.default_state(),
            UpdateFlags(82),
            UPDATE_LIMIT,
        )?;
    }
    for (&pos, &old) in delete.iter() {
        indirect_shapes(level, old, pos)?;
        level.update_neighbor_shapes(
            Block::AIR.default_state(),
            pos,
            UpdateFlags::CLIENTS,
            UPDATE_LIMIT,
        )?;
        indirect_shapes(level, Block::AIR.default_state(), pos)?;
    }
    let orientation = initial_orientation(level, Some(resolver.push_direction), None);
    let mut update_index = 0;
    for &pos in resolver.to_destroy.iter().rev() {
        let state = to_update[update_index];
        update_index += 1;
        level.optional_block_call(
            state.block(),
            BlockMethod::AffectNeighborsAfterRemoval,
            |behavior, level| behavior.affect_neighbors_after_removal(state, pos, false, level),
        )?;
        indirect_shapes(level, state, pos)?;
        level.update_neighbors_at_except_from_facing(pos, state.block(), None, orientation)?;
    }
    for &pos in resolver.to_push.iter().rev() {
        level.update_neighbors_at_except_from_facing(
            pos,
            to_update[update_index].block(),
            None,
            orientation,
        )?;
        update_index += 1;
    }
    if extending {
        level.update_neighbors_at_except_from_facing(arm, Block::PISTON_HEAD, None, orientation)?;
    }
    Ok(true)
}
impl BlockBehavior for PistonBaseBlock {
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<crate::entity::EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        check_if_extend(level, pos, state)
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() && level.block_entity_ref(pos)?.is_none() {
            check_if_extend(level, pos, state)?;
        }
        Ok(())
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        check_if_extend(level, pos, state)
    }
    fn trigger_event(
        &self,
        state: BlockState,
        pos: BlockPos,
        event: i32,
        data: i32,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        let direction = facing(state);
        let extended = state.with(properties::EXTENDED, true).unwrap();
        let extend = neighbor_signal(level, pos, direction)?;
        if extend && matches!(event, 1 | 2) {
            level.set_block(pos, extended, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
            return Ok(false);
        }
        if !extend && event == 0 {
            return Ok(false);
        }
        let sticky = state.block() == Block::STICKY_PISTON;
        if event == 0 {
            if !move_blocks(level, pos, direction, true, sticky)? {
                return Ok(false);
            }
            level.set_block(pos, extended, UpdateFlags(67), UPDATE_LIMIT)?;
            let pitch = level.random().next_float() * 0.25 + 0.6;
            level.play_sound(
                None,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                SoundEvent::BLOCK_PISTON_EXTEND,
                SoundSource::Blocks,
                0.5,
                pitch,
            )?;
            level.game_event(
                GameEvent::BLOCK_ACTIVATE,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                None,
                Some(extended),
            );
        } else if matches!(event, 1 | 2) {
            if let Some(handle) = moving_entity_at(level, pos.relative(direction, 1))? {
                moving_piston::final_tick(level, handle)?;
            }
            let kind = if sticky {
                properties::Type::Sticky
            } else {
                properties::Type::Normal
            };
            let actual = with_facing(
                Block::MOVING_PISTON
                    .default_state()
                    .with(properties::TYPE_, kind)
                    .unwrap(),
                direction,
            );
            level.set_block(pos, actual, UpdateFlags(276), UPDATE_LIMIT)?;
            let moved = with_facing(
                state.block().default_state(),
                Direction::from_3d_data_value(data & 7),
            );
            set_moving_entity(level, pos, actual, moved, direction, false, true)?;
            level.update_neighbors_at(pos, actual.block())?;
            level.update_neighbor_shapes(actual, pos, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
            if sticky {
                let two = pos.relative(direction, 2);
                let moving = level.get_block_state(two)?;
                let mut piece = false;
                if moving.block() == Block::MOVING_PISTON
                    && let Some(handle) = moving_entity_at(level, two)?
                {
                    let entity = level.block_entity::<PistonMovingBlockEntity>(handle)?;
                    if entity.direction == direction && entity.extending {
                        moving_piston::final_tick(level, handle)?;
                        piece = true;
                    }
                }
                if !piece {
                    if event != 1
                        || moving.is_air()
                        || !is_pushable(moving, level, two, direction.opposite(), false, direction)
                        || (moving.data().push_reaction != PushReaction::PushPull
                            && !matches!(moving.block(), Block::PISTON | Block::STICKY_PISTON))
                    {
                        level.remove_block(pos.relative(direction, 1), false)?;
                    } else {
                        move_blocks(level, pos, direction, false, sticky)?;
                    }
                }
            } else {
                level.remove_block(pos.relative(direction, 1), false)?;
            }
            let pitch = level.random().next_float() * 0.15 + 0.6;
            level.play_sound(
                None,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                SoundEvent::BLOCK_PISTON_CONTRACT,
                SoundSource::Blocks,
                0.5,
                pitch,
            )?;
            level.game_event(
                GameEvent::BLOCK_DEACTIVATE,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                None,
                Some(actual),
            );
        }
        Ok(true)
    }
}

use crate::block::{LevelAccessRef, LevelReaderRef};
pub struct PistonHeadBlock;
pub static PISTON_HEAD: PistonHeadBlock = PistonHeadBlock;
fn fitting_base(head: BlockState, base: BlockState) -> bool {
    let block = if head.get(properties::TYPE_).unwrap() == properties::Type::Normal {
        Block::PISTON
    } else {
        Block::STICKY_PISTON
    };
    base.block() == block && base.get(properties::EXTENDED).unwrap() && facing(base) == facing(head)
}
impl BlockBehavior for PistonHeadBlock {
    /// Port of PistonHeadBlock#playerWillDestroy. Creative removal destroys a
    /// fitting base before the head's inherited particles and game event.
    fn player_will_destroy(
        &self,
        state: BlockState,
        pos: BlockPos,
        player: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<BlockState, LevelError> {
        if level
            .entity::<crate::entity::player::ServerPlayer>(player)?
            .prevents_block_drops()
        {
            let base = pos.relative(facing(state).opposite(), 1);
            if fitting_base(state, level.get_block_state(base)?) {
                level.destroy_block(base, false, UPDATE_LIMIT)?;
            }
        }
        crate::block::BASE_BLOCK.player_will_destroy(state, pos, player, level)
    }

    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let base = reader.get_block_state(pos.relative(facing(state).opposite(), 1))?;
        Ok(fitting_base(state, base)
            || (base.block() == Block::MOVING_PISTON && facing(base) == facing(state)))
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        Ok(
            if direction.opposite() == facing(state)
                && !self.can_survive(state, pos, &level.as_reader())?
            {
                Block::AIR.default_state()
            } else {
                state
            },
        )
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
        if self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
            let direction = facing(state).opposite();
            level.neighbor_changed(
                pos.relative(direction, 1),
                source,
                orientation.map(|o| o.with_front(direction)),
            )?;
        }
        Ok(())
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let base = pos.relative(facing(state).opposite(), 1);
        if fitting_base(state, level.get_block_state(base)?) {
            level.destroy_block(base, true, UPDATE_LIMIT)?;
        }
        Ok(())
    }
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _include_data: bool,
        _reader: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_protocol::component::ItemStack, LevelError> {
        Ok(ferraloom_protocol::component::ItemStack::new(
            if state.get(properties::TYPE_).unwrap() == properties::Type::Sticky {
                Block::STICKY_PISTON
            } else {
                Block::PISTON
            }
            .as_item(),
            1,
            ferraloom_protocol::component::DataComponentPatch::default(),
        ))
    }
}
