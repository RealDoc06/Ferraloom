//! Port of ComparatorBlockEntity's persisted output signal.
use crate::block::entity::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::{CompoundTag, Tag};

#[derive(Clone, Debug)]
pub struct ComparatorBlockEntity {
    pub parent: BlockEntityData,
    pub output: i32,
}
impl ComparatorBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self { parent, output: 0 }
    }
}

pub struct ComparatorEntityBehavior;
pub static COMPARATOR_ENTITY: ComparatorEntityBehavior = ComparatorEntityBehavior;
impl BlockEntity for ComparatorEntityBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        if let Some(entity) =
            <ComparatorBlockEntity as crate::block::entity::BlockEntityDataView>::view_mut(state)
        {
            entity.output = tag.get_int_or("OutputSignal", 0);
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = CompoundTag::new();
        if let Some(entity) =
            <ComparatorBlockEntity as crate::block::entity::BlockEntityDataView>::view(state)
        {
            tag.insert("OutputSignal", Tag::Int(entity.output));
        }
        Ok(tag)
    }
}

use super::{diode, state::facing};
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError, redstone::Orientation},
    ticks::TickPriority,
};
use ferraloom_data::{Block, BlockState, EntityType, properties as p};
use ferraloom_util::{Aabb, BlockPos, Direction};

pub struct ComparatorBlock;
pub static COMPARATOR: ComparatorBlock = ComparatorBlock;
fn output_signal(level: &Level, pos: BlockPos) -> Result<i32, LevelError> {
    output_signal_from_reader(&LevelReaderRef::Server(level), pos)
}
fn output_signal_from_reader(
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<i32, LevelError> {
    let Some(handle) = reader.block_entity_ref(pos)? else {
        return Ok(0);
    };
    Ok(match reader.block_entity_state(handle)? {
        BlockEntityState::Comparator(entity) => entity.output,
        _ => 0,
    })
}
fn input_signal(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<i32, LevelError> {
    let mut result = diode::input_signal(level, pos, state)?;
    let direction = facing(state);
    let mut target = pos.relative(direction, 1);
    let mut target_state = level.get_block_state(target)?;
    if target_state.has_analog_output_signal() {
        result = level.get_analog_output_signal(target_state, target, direction.opposite())?;
    } else if result < 15 && level.is_redstone_conductor(target_state, target)? {
        target = target.relative(direction, 1);
        target_state = level.get_block_state(target)?;
        let box_ = Aabb::new(
            f64::from(target.x),
            f64::from(target.y),
            f64::from(target.z),
            f64::from(target.x + 1),
            f64::from(target.y + 1),
            f64::from(target.z + 1),
        );
        let frames = level.entities_in(box_, |_, entity| {
            matches!(
                entity.entity_type(),
                EntityType::ITEM_FRAME | EntityType::GLOW_ITEM_FRAME
            )
        });
        if let Some(frame) = frames.first() {
            return Err(LevelError::UnsupportedEntity(
                level.entity_state(*frame)?.entity_type(),
            ));
        }
        if target_state.has_analog_output_signal() {
            result = level.get_analog_output_signal(target_state, target, direction.opposite())?;
        }
    }
    Ok(result)
}
fn alternate_signal(level: &Level, pos: BlockPos, state: BlockState) -> Result<i32, LevelError> {
    diode::alternate_signal(&LevelReaderRef::Server(level), pos, state, false)
}
pub(super) fn calculate_output(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
) -> Result<i32, LevelError> {
    let input = input_signal(level, pos, state)?;
    if input == 0 {
        return Ok(0);
    }
    let alternate = alternate_signal(level, pos, state)?;
    if alternate > input {
        Ok(0)
    } else if state.get(p::MODE).unwrap() == p::Mode::Subtract {
        Ok(input - alternate)
    } else {
        Ok(input)
    }
}
pub(super) fn should_turn_on(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
) -> Result<bool, LevelError> {
    let input = input_signal(level, pos, state)?;
    if input == 0 {
        return Ok(false);
    }
    let alternate = alternate_signal(level, pos, state)?;
    Ok(
        input > alternate
            || (input == alternate && state.get(p::MODE).unwrap() == p::Mode::Compare),
    )
}
fn refresh_output(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let output = calculate_output(level, pos, state)?;
    let mut old = 0;
    if let Some(handle) = level.block_entity_ref(pos)?
        && let BlockEntityState::Comparator(entity) = level.block_entity_state_mut(handle)?
    {
        old = entity.output;
        entity.output = output;
    }
    if old != output || state.get(p::MODE).unwrap() == p::Mode::Compare {
        let source = should_turn_on(level, pos, state)?;
        let on = state.get(p::POWERED).unwrap();
        if on != source {
            level.set_block(
                pos,
                state.with(p::POWERED, source).unwrap(),
                UpdateFlags::CLIENTS,
                UPDATE_LIMIT,
            )?;
        }
        diode::update_front(level, pos, state)?;
    }
    Ok(())
}
impl BlockBehavior for ComparatorBlock {
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<crate::entity::EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if should_turn_on(level, pos, state)? {
            level.schedule_block_tick_after(pos, state.block(), 1)?;
        }
        Ok(())
    }
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &crate::block::UseContext,
        level: &mut Level,
    ) -> Result<crate::block::InteractionResult, LevelError> {
        use crate::{block::InteractionResult, entity::player::ServerPlayer};
        use ferraloom_data::registries::sound_event::Id as SoundEvent;
        use ferraloom_protocol::play::SoundSource;
        use ferraloom_util::Vec3;
        if !level.entity::<ServerPlayer>(context.player)?.may_build() {
            return Ok(InteractionResult::Pass);
        }
        let mode = if state.get(p::MODE).unwrap() == p::Mode::Compare {
            p::Mode::Subtract
        } else {
            p::Mode::Compare
        };
        let changed = state.with(p::MODE, mode).unwrap();
        level.play_sound(
            Some(context.player),
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            SoundEvent::BLOCK_COMPARATOR_CLICK,
            SoundSource::Blocks,
            0.3,
            if mode == p::Mode::Subtract { 0.55 } else { 0.5 },
        )?;
        level.set_block(pos, changed, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
        if level.get_block_state(pos)?.block() == state.block() {
            refresh_output(level, pos, changed)?;
        }
        Ok(InteractionResult::SUCCESS)
    }

    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if !state.get(p::POWERED).unwrap() {
            return Ok(0);
        }
        match reader {
            LevelReaderRef::Server(level) => output_signal(level, pos),
            LevelReaderRef::PathNavigation(_) => output_signal_from_reader(reader, pos),
            LevelReaderRef::Worldgen(_) => {
                Err(LevelError::UnsupportedBlockEntity(Block::COMPARATOR))
            }
        }
    }
    fn signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if facing(state) == direction {
            self.own_signal(state, pos, reader)
        } else {
            Ok(0)
        }
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        self.signal(state, pos, direction, reader)
    }
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = pos.below(1);
        diode::survives_on(reader, below, reader.get_block_state(below)?)
    }
    fn update_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        Ok(
            if direction == Direction::Down
                && !diode::survives_on(&level.as_reader(), neighbor_pos, neighbor)?
            {
                Block::AIR.default_state()
            } else {
                state
            },
        )
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        diode::update_front(level, pos, state)
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !moved {
            diode::update_front(level, pos, state)?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        refresh_output(level, pos, state)
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
        if level.get_block_state(pos)?.block() != state.block() {
            return Ok(());
        }
        if !self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
            let block_entity = if state.has_block_entity() {
                level.get_or_promote_block_entity(pos)?
            } else {
                None
            };
            crate::block::drops::drop_resources(state, pos, block_entity, level)?;
            level.remove_block(pos, false)?;
            for d in Direction::VALUES {
                level.update_neighbors_at(pos.relative(d, 1), state.block())?;
            }
            return Ok(());
        }
        if !level.will_tick_block_this_tick(pos, state.block()) {
            let value = calculate_output(level, pos, state)?;
            let old = output_signal(level, pos)?;
            if value != old || state.get(p::POWERED).unwrap() != should_turn_on(level, pos, state)?
            {
                let priority =
                    if diode::should_prioritize(&LevelReaderRef::Server(level), pos, state)? {
                        TickPriority::High
                    } else {
                        TickPriority::Normal
                    };
                level.schedule_block_tick(
                    pos,
                    state.block(),
                    level.game_time().wrapping_add(2),
                    priority,
                )?;
            }
        }
        Ok(())
    }
    fn trigger_event(
        &self,
        _state: BlockState,
        pos: BlockPos,
        a: i32,
        b: i32,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(false);
        };
        let behavior = level.block_entity_state(handle)?.behavior();
        Ok(behavior.trigger_event(level.block_entity_state_mut(handle)?, a, b))
    }
}
