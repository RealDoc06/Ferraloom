//! Player use and scheduled lifecycle of non-container redstone devices.
use crate::{
    block::entity::{BlockEntityState, BlockEntityType},
    block::{
        BlockBehavior, BlockMethod, InteractionResult, LevelAccessRef, UPDATE_LIMIT, UpdateFlags,
        UseContext,
    },
    entity::player::ServerPlayer,
    level::{Level, LevelError, environment::EnvironmentAttributes},
    stats::{self, Stat, custom},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p, registries::fluid::Id as Fluid,
};
use ferraloom_util::{BlockPos, Direction, Vec3, mth};

pub struct Devices;
pub static DEVICES: Devices = Devices;
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    use BlockMethod as M;
    match block.class() {
        BlockClass::DispenserBlock | BlockClass::DropperBlock | BlockClass::HopperBlock => {
            method == M::UseWithoutItem
        }
        BlockClass::DaylightDetectorBlock => method == M::UseWithoutItem,
        BlockClass::LightningRodBlock | BlockClass::WeatheringLightningRodBlock => matches!(
            method,
            M::OnPlace
                | M::Tick
                | M::AffectNeighborsAfterRemoval
                | M::UpdateShape
                | M::UpdateFromNeighbourShapes
        ),
        _ => false,
    }
}
fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}
/// Port of DaylightDetectorBlock#updateSignalStrength's float calculation.
/// SUN_ANGLE is in degrees; the native adjustment precedes Mth's lookup-table cosine.
pub(super) fn daylight_power(brightness: i32, angle: f32, inverted: bool) -> i32 {
    if inverted {
        return 15_i32.wrapping_sub(brightness).clamp(0, 15);
    }
    if brightness <= 0 {
        return brightness.clamp(0, 15);
    }
    let mut angle = angle * mth::DEG_TO_RAD;
    let offset = if angle < mth::PI { 0.0 } else { mth::TWO_PI };
    angle += (offset - angle) * 0.2;
    let value = brightness as f32 * mth::cos(f64::from(angle));
    (f64::from(value) + 0.5).floor().clamp(0.0, 15.0) as i32
}
/// Port of DaylightDetectorBlock#updateSignalStrength. Both required environment
/// reads occur even for inverted detectors, in vanilla's order.
pub fn update_daylight_signal(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    let brightness = level.get_effective_sky_brightness(pos)?;
    let angle = level.get_value(EnvironmentAttributes::SUN_ANGLE, pos)?;
    let power = daylight_power(brightness, angle, state.get(p::INVERTED).unwrap());
    if i32::from(state.get(p::POWER).unwrap()) != power {
        level.set_block_and_update(pos, state.with(p::POWER, power as u8).unwrap())?;
    }
    Ok(())
}
/// Port of DaylightDetectorBlock#tickEntity. Ticker selection, including the
/// static dimension skylight gate, belongs to the canonical BE framework.
pub fn tick_daylight_detector(
    receiver: crate::block::entity::BlockEntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.game_time() % 20 == 0 {
        let state = level
            .block_entity::<crate::block::entity::BlockEntityData>(receiver)?
            .block_state;
        update_daylight_signal(state, receiver.pos, level)?;
    }
    Ok(())
}
fn rod_neighbors(state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    let front = super::state::facing(state).opposite();
    let orientation = super::initial_orientation(level, Some(front), None);
    level.update_neighbors_at_except_from_facing(
        pos.relative(front, 1),
        state.block(),
        None,
        orientation,
    )
}
/// Port of LightningRodBlock#onLightningStrike. The lightning entity calls this
/// exact block-side hook; weather/entity targeting remains with that family.
pub fn on_lightning_strike(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.set_block_and_update(pos, state.with(p::POWERED, true).unwrap())?;
    rod_neighbors(state, pos, level)?;
    level.schedule_tick(pos, state.block(), 8)?;
    level.level_event(None, 3002, pos, super::state::facing(state).axis() as i32)?;
    Ok(())
}
impl BlockBehavior for Devices {
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        if state.block() == Block::DAYLIGHT_DETECTOR {
            if !level.entity::<ServerPlayer>(context.player)?.may_build() {
                return Ok(InteractionResult::Pass);
            }
            let changed = state
                .with(p::INVERTED, !state.get(p::INVERTED).unwrap())
                .unwrap();
            level.set_block(pos, changed, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
            level.game_event(
                ferraloom_data::registries::game_event::Id::BLOCK_CHANGE,
                center(pos),
                Some(context.player),
                Some(changed),
            )?;
            update_daylight_signal(changed, pos, level)?;
            return Ok(InteractionResult::SUCCESS);
        }
        // Native instanceof tests the block entity captured at entry. The stat
        // and menu kind follow that object, including a dropper at a dispenser.
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(InteractionResult::SUCCESS);
        };
        let kind = match level.block_entity_state(handle)? {
            BlockEntityState::Hopper(_) if state.block() == Block::HOPPER => {
                BlockEntityType::HOPPER
            }
            BlockEntityState::Dispenser(_) if state.block() != Block::HOPPER => {
                level.block_entity_state(handle)?.data().kind
            }
            _ => return Ok(InteractionResult::SUCCESS),
        };
        let provider_state = match kind {
            BlockEntityType::HOPPER => Block::HOPPER,
            BlockEntityType::DROPPER => Block::DROPPER,
            _ => Block::DISPENSER,
        }
        .default_state();
        let Some(provider) = crate::block::containers::menu::provider(provider_state, pos, level)?
        else {
            return Ok(InteractionResult::SUCCESS);
        };
        crate::inventory::open_menu_live(context.player, &provider, level)
            .map_err(super::stack_error)?;
        let stat = match kind {
            BlockEntityType::HOPPER => custom::INSPECT_HOPPER,
            BlockEntityType::DROPPER => custom::INSPECT_DROPPER,
            _ => custom::INSPECT_DISPENSER,
        };
        stats::award_stat(level, context.player, Stat::Custom(stat), 1)?;
        Ok(InteractionResult::SUCCESS)
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block()
            && state.get(p::POWERED).unwrap()
            && !level.has_scheduled_block_tick(pos, state.block())
        {
            level.schedule_tick(pos, state.block(), 8)?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        level.set_block_and_update(pos, state.with(p::POWERED, false).unwrap())?;
        rod_neighbors(state, pos, level)
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.get(p::POWERED).unwrap() {
            rod_neighbors(state, pos, level)?;
        }
        Ok(())
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
            level.schedule_fluid_tick(pos, Fluid::WATER, 5)?;
        }
        Ok(state)
    }
}
