//! Ports of dripleaf placement and bonemeal growth shared with generation.
use super::{
    super::{LevelAccessRef, LevelReaderRef, UpdateFlags, fluid},
    bonemeal::inside_height,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, random::RandomSource};

pub(super) fn can_grow_into(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<bool, LevelError> {
    let state = level.get_block_state(pos)?;
    Ok(inside_height(level, pos)
        && (state.is_air() || matches!(state.block(), Block::WATER | Block::SMALL_DRIPLEAF)))
}

fn place(
    block: Block,
    facing: p::Facing,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let waterlogged = super::source_water(level.as_reader().get_fluid_state(pos)?);
    let state = block
        .default_state()
        .with(p::WATERLOGGED, waterlogged)
        .unwrap()
        .with(p::FACING, facing)
        .unwrap();
    level.set_block(pos, state, UpdateFlags::ALL)?;
    Ok(())
}

/// Port of BigDripleafBlock#placeWithRandomHeight. Even zero available height
/// places the leaf at the original position, as vanilla does.
pub(super) fn place_with_random_height(
    pos: BlockPos,
    facing: p::Facing,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let desired = level.random()?.next_int_bound(4) + 2;
    let mut height = 0;
    while height < desired && can_grow_into(&level.as_reader(), pos.above(height))? {
        height += 1;
    }
    let mut y = 0;
    while y < height - 1 {
        place(Block::BIG_DRIPLEAF_STEM, facing, pos.above(y), level)?;
        y += 1;
    }
    place(Block::BIG_DRIPLEAF, facing, pos.above(y), level)
}

pub(super) fn head_pos(
    state: BlockState,
    mut pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<Option<BlockPos>, LevelError> {
    if state.block() == Block::BIG_DRIPLEAF {
        return Ok(Some(pos));
    }
    loop {
        pos = pos.above(1);
        let forward = level.get_block_state(pos)?;
        if forward.block() != state.block() {
            return Ok((forward.block() == Block::BIG_DRIPLEAF).then_some(pos));
        }
    }
}

pub(super) fn bonemeal(
    mut state: BlockState,
    mut pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    if state.block() == Block::SMALL_DRIPLEAF {
        while state.get(p::HALF) == Some(p::Half::Upper) {
            pos = pos.below(1);
            state = level.get_block_state(pos)?;
        }
        let above = pos.above(1);
        let replaced = fluid::legacy_block(level.as_reader().get_fluid_state(above)?);
        level.set_block(above, replaced, UpdateFlags(18))?;
        return place_with_random_height(pos, state.get(p::FACING).unwrap(), level);
    }
    if let Some(head) = head_pos(state, pos, &level.as_reader())?
        && (state.block() == Block::BIG_DRIPLEAF_STEM
            || can_grow_into(&level.as_reader(), head.above(1))?)
    {
        let facing = state.get(p::FACING).unwrap();
        place(Block::BIG_DRIPLEAF_STEM, facing, head, level)?;
        place(Block::BIG_DRIPLEAF, facing, head.above(1), level)?;
    }
    Ok(())
}

pub struct Dripleaf;
pub static DRIPLEAF: Dripleaf = Dripleaf;

/// Port of BigDripleafBlock#setTilt, including vibration only on a real transition.
fn set_tilt(
    state: BlockState,
    pos: BlockPos,
    tilt: p::Tilt,
    level: &mut crate::level::Level,
) -> Result<(), LevelError> {
    let previous = state.get(p::TILT).unwrap();
    LevelAccessRef::Server(level).set_block(
        pos,
        state.with(p::TILT, tilt).unwrap(),
        UpdateFlags::CLIENTS,
    )?;
    if tilt != p::Tilt::Unstable && tilt != previous {
        level.game_event(
            ferraloom_data::registries::game_event::Id::BLOCK_CHANGE,
            ferraloom_util::Vec3::at_center_of(pos.into()),
            None,
            None,
        )?;
    }
    Ok(())
}
fn tilt_sound(
    pos: BlockPos,
    sound: ferraloom_data::registries::sound_event::Id,
    level: &mut crate::level::Level,
) -> Result<(), LevelError> {
    let pitch = 0.8_f32 + level.random().next_float() * (1.2_f32 - 0.8_f32);
    level.play_sound(
        None,
        ferraloom_util::Vec3::at_center_of(pos.into()),
        sound,
        ferraloom_protocol::play::SoundSource::Blocks,
        1.0,
        pitch,
    )
}
fn reset_tilt(
    state: BlockState,
    pos: BlockPos,
    level: &mut crate::level::Level,
) -> Result<(), LevelError> {
    set_tilt(state, pos, p::Tilt::None, level)?;
    if state.get(p::TILT) != Some(p::Tilt::None) {
        tilt_sound(
            pos,
            ferraloom_data::registries::sound_event::Id::BLOCK_BIG_DRIPLEAF_TILT_UP,
            level,
        )?;
    }
    Ok(())
}
fn tilt_and_schedule(
    state: BlockState,
    pos: BlockPos,
    tilt: p::Tilt,
    level: &mut crate::level::Level,
) -> Result<(), LevelError> {
    set_tilt(state, pos, tilt, level)?;
    if tilt != p::Tilt::Unstable {
        tilt_sound(
            pos,
            ferraloom_data::registries::sound_event::Id::BLOCK_BIG_DRIPLEAF_TILT_DOWN,
            level,
        )?;
    }
    let delay = match tilt {
        p::Tilt::Unstable | p::Tilt::Partial => 10,
        p::Tilt::Full => 100,
        p::Tilt::None => return Ok(()),
    };
    level.schedule_block_tick_after(pos, state.block(), delay)
}
impl super::super::BlockBehavior for Dripleaf {
    fn tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut crate::level::Level,
    ) -> Result<(), LevelError> {
        if LevelReaderRef::Server(level).has_neighbor_signal(pos)? {
            return reset_tilt(state, pos, level);
        }
        match state.get(p::TILT).unwrap() {
            p::Tilt::Unstable => tilt_and_schedule(state, pos, p::Tilt::Partial, level),
            p::Tilt::Partial => tilt_and_schedule(state, pos, p::Tilt::Full, level),
            p::Tilt::Full => reset_tilt(state, pos, level),
            p::Tilt::None => Ok(()),
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<super::super::Orientation>,
        _moved: bool,
        level: &mut crate::level::Level,
    ) -> Result<(), LevelError> {
        if LevelReaderRef::Server(level).has_neighbor_signal(pos)? {
            reset_tilt(state, pos, level)?;
        }
        Ok(())
    }
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: crate::entity::EntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), LevelError> {
        if state.get(p::TILT) == Some(p::Tilt::None) {
            let entity = level.entity::<crate::entity::EntityData>(entity)?;
            if entity.on_ground
                && entity.position.y > f64::from(pos.y as f32 + 0.6875_f32)
                && !LevelReaderRef::Server(level).has_neighbor_signal(pos)?
            {
                tilt_and_schedule(state, pos, p::Tilt::Unstable, level)?;
            }
        }
        Ok(())
    }
}
