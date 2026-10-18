//! DoorBlock, TrapDoorBlock and FenceGateBlock redstone and player interactions.
use super::state::{facing, with_facing};
use crate::{
    block::{
        BlockBehavior, InteractionResult, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT,
        UpdateFlags, UseContext,
    },
    entity::{EntityData, EntityRef},
    level::{Level, LevelError, redstone::Orientation},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p,
    registries::{fluid::Id as Fluid, game_event::Id as GameEvent, sound_event::Id as SoundEvent},
    tags::BlockTags,
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{Axis, BlockPos, Direction, Vec3, VoxelShape, random::RandomSource, shapes};
pub struct OpeningBlock;
pub static OPENING: OpeningBlock = OpeningBlock;
fn is_door(state: BlockState) -> bool {
    state.block().is_instance_of(BlockClass::DoorBlock)
}
fn is_gate(state: BlockState) -> bool {
    state.block().is_instance_of(BlockClass::FenceGateBlock)
}
fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}
fn play_sound(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    open: bool,
    source: Option<EntityRef>,
) -> Result<(), LevelError> {
    let facts = state.block().redstone_properties();
    let sound = if is_gate(state) {
        if open {
            facts.gate_open.unwrap()
        } else {
            facts.gate_close.unwrap()
        }
    } else {
        let set = facts.block_set.unwrap();
        if is_door(state) {
            if open { set.door_open } else { set.door_close }
        } else if open {
            set.trapdoor_open
        } else {
            set.trapdoor_close
        }
    };
    let pitch = level.random().next_float() * 0.1 + 0.9;
    level.play_sound(source, center(pos), sound, SoundSource::Blocks, 1.0, pitch)
}
fn event(
    level: &mut Level,
    pos: BlockPos,
    open: bool,
    source: Option<EntityRef>,
) -> Result<(), LevelError> {
    level.game_event(
        if open {
            GameEvent::BLOCK_OPEN
        } else {
            GameEvent::BLOCK_CLOSE
        },
        center(pos),
        source,
        None,
    )
}
fn schedule_water(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    if state.get(p::WATERLOGGED) == Some(true) {
        level.schedule_fluid_tick_after(pos, Fluid::WATER, 5)?;
    }
    Ok(())
}
impl BlockBehavior for OpeningBlock {
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<crate::entity::EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.set_block_and_update(pos.above(1), state.with(p::HALF, p::Half::Upper).unwrap())?;
        Ok(())
    }
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        if !is_gate(state)
            && !state
                .block()
                .redstone_properties()
                .block_set
                .unwrap()
                .can_open_by_hand
        {
            return Ok(InteractionResult::Pass);
        }
        let open = !state.get(p::OPEN).unwrap();
        let mut changed = state.with(p::OPEN, open).unwrap();
        if is_gate(state) && open {
            let direction =
                Direction::from_y_rot(f64::from(level.entity::<EntityData>(context.player)?.yaw));
            if facing(state) == direction.opposite() {
                changed = with_facing(changed, direction);
            }
        }
        level.set_block(
            pos,
            changed,
            UpdateFlags(if is_door(state) || is_gate(state) {
                10
            } else {
                2
            }),
            UPDATE_LIMIT,
        )?;
        if !is_door(state) && !is_gate(state) {
            schedule_water(level, pos, changed)?;
        }
        play_sound(level, pos, changed, open, Some(context.player))?;
        event(level, pos, open, Some(context.player))?;
        Ok(InteractionResult::SUCCESS)
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let signal = level.has_neighbor_signal(pos)?
            || (is_door(state)
                && level.has_neighbor_signal(pos.relative(
                    if state.get(p::HALF).unwrap() == p::Half::Lower {
                        Direction::Up
                    } else {
                        Direction::Down
                    },
                    1,
                ))?);
        if is_door(state) && source == state.block() {
            return Ok(());
        }
        if signal != state.get(p::POWERED).unwrap() {
            let changes_open = signal != state.get(p::OPEN).unwrap();
            if !is_gate(state) && changes_open {
                play_sound(level, pos, state, signal, None)?;
                event(level, pos, signal, None)?;
            }
            let changed = state
                .with(p::POWERED, signal)
                .unwrap()
                .with(p::OPEN, signal)
                .unwrap();
            level.set_block(pos, changed, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
            if is_gate(state) && changes_open {
                play_sound(level, pos, state, signal, None)?;
                event(level, pos, signal, None)?;
            }
            if !is_door(state) && !is_gate(state) {
                schedule_water(level, pos, state)?;
            }
        }
        Ok(())
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if !is_door(state) {
            return Ok(true);
        }
        let below = pos.below(1);
        let below_state = reader.get_block_state(below)?;
        if state.get(p::HALF).unwrap() == p::Half::Lower {
            reader.is_face_sturdy(below_state, below, Direction::Up)
        } else {
            Ok(below_state.block() == state.block())
        }
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if is_door(state) {
            let half = state.get(p::HALF).unwrap();
            if direction.axis() != Axis::Y
                || ((half == p::Half::Lower) != (direction == Direction::Up))
            {
                Ok(
                    if half == p::Half::Lower
                        && direction == Direction::Down
                        && !self.can_survive(state, pos, &level.as_reader())?
                    {
                        Block::AIR.default_state()
                    } else {
                        state
                    },
                )
            } else {
                Ok(
                    if neighbor.block().is_instance_of(BlockClass::DoorBlock)
                        && neighbor.get(p::HALF).unwrap() != half
                    {
                        neighbor.with(p::HALF, half).unwrap()
                    } else {
                        Block::AIR.default_state()
                    },
                )
            }
        } else if is_gate(state) {
            if facing(state).clockwise().axis() != direction.axis() {
                return Ok(state);
            }
            let reader = level.as_reader();
            let tags = reader.block_tags()?;
            let in_wall = tags.contains_key(BlockTags::WALLS, neighbor.block())
                || tags.contains_key(
                    BlockTags::WALLS,
                    reader
                        .get_block_state(pos.relative(direction.opposite(), 1))?
                        .block(),
                );
            Ok(state.with(p::IN_WALL, in_wall).unwrap())
        } else {
            if state.get(p::WATERLOGGED).unwrap() {
                level.schedule_fluid_tick(pos, Fluid::WATER, 5)?;
            }
            Ok(state)
        }
    }
    fn support_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        if !is_gate(state) {
            return reader.collision_shape_without_context(state, pos);
        }
        if state.get(p::OPEN).unwrap() {
            return Ok(shapes::empty());
        }
        Ok(if facing(state).axis() == Axis::Z {
            shapes::box_shape(0.0, 5.0 / 16.0, 6.0 / 16.0, 1.0, 1.5, 10.0 / 16.0)
        } else {
            shapes::box_shape(6.0 / 16.0, 5.0 / 16.0, 0.0, 10.0 / 16.0, 1.5, 1.0)
        })
    }
}

/// Port of CopperBulbBlock and its weathering subclass. Analog output consults
/// the live state at pos even if the caller supplied a captured prior state.
pub struct CopperBulbBlock;
pub static BULB: CopperBulbBlock = CopperBulbBlock;
fn check_bulb(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let signal = level.has_neighbor_signal(pos)?;
    if signal != state.get(p::POWERED).unwrap() {
        let mut changed = state;
        if !state.get(p::POWERED).unwrap() {
            let lit = !state.get(p::LIT).unwrap();
            changed = state.with(p::LIT, lit).unwrap();
            level.play_sound(
                None,
                center(pos),
                if lit {
                    SoundEvent::BLOCK_COPPER_BULB_TURN_ON
                } else {
                    SoundEvent::BLOCK_COPPER_BULB_TURN_OFF
                },
                SoundSource::Blocks,
                1.0,
                1.0,
            )?;
        }
        level.set_block(
            pos,
            changed.with(p::POWERED, signal).unwrap(),
            UpdateFlags::ALL,
            UPDATE_LIMIT,
        )?;
    }
    Ok(())
}
impl BlockBehavior for CopperBulbBlock {
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() {
            check_bulb(level, pos, state)?;
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
        check_bulb(level, pos, state)
    }
    fn has_analog_output_signal(&self, _state: BlockState) -> bool {
        true
    }
    fn analog_output_signal(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        level: &mut Level,
    ) -> Result<i32, LevelError> {
        Ok(if level.get_block_state(pos)?.get(p::LIT).unwrap() {
            15
        } else {
            0
        })
    }
}
