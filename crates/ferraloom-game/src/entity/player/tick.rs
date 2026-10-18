//! The connection's ServerPlayer#doTick, separate from the entity-phase tick.
use super::ServerPlayer;
use crate::{entity::synched_data as data, level::effects::LevelEffect, stats::Stat};
use crate::{
    entity::{Entity, EntityData, EntityRef},
    level::{Level, LevelError, tick_rate::TickRateManager},
};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_protocol::{codec::VarInt, play::clientbound as cb};
use ferraloom_util::{ChunkPos, mth};

/// Port of Entity#touchingUnloadedChunk. This is a loaded-presence query; it
/// never acquires a missing chunk or substitutes a block-state read.
fn touching_unloaded_chunk(player: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let bounds = level
        .entity::<EntityData>(player)?
        .bounding_box
        .inflate(1.0, 1.0, 1.0);
    for x in mth::floor(bounds.min_x) >> 4..=mth::ceil(bounds.max_x) >> 4 {
        for z in mth::floor(bounds.min_z) >> 4..=mth::ceil(bounds.max_z) >> 4 {
            if level.chunk(ChunkPos::new(x, z)).is_none() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The currently ported Player#tick parent body. It runs once at connection
/// doTick, while ServerPlayer#tick retains its distinct entity-phase behavior.
fn player_tick<E: Entity + ?Sized>(
    behavior: &E,
    player: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    let spectator = behavior.is_spectator(player, level)?;
    let passenger = level
        .entity::<ServerPlayer>(player)?
        .vehicle
        .get()
        .is_some();
    level.entity_mut::<EntityData>(player)?.no_physics = spectator;
    if spectator || passenger {
        crate::entity::movement::set_on_ground(player, false, None, level)?;
    }
    crate::entity::base_tick(behavior, player, level)?;
    super::ai_step::ai_step(behavior, player, level, tick_rate)?;
    // LivingEntity#tick advances flight time after the virtual aiStep returns.
    let living = level.entity_mut::<crate::entity::LivingEntity>(player)?;
    living.fall_fly_ticks = if living.shared_flag(7) {
        living.fall_fly_ticks.wrapping_add(1)
    } else {
        0
    };
    super::cooldowns::tick(player, level)
}

/// Port of ServerPlayer#doTick. The caller resets movement bookkeeping before
/// entering, then absSnapTo restores firstGood with the current rotation.
pub fn player_do_tick_with_tick_rate(
    player: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    let behavior = level.entity_behavior(player)?;
    if !behavior.is_spectator(player, level)? || !touching_unloaded_chunk(player, level)? {
        player_tick(behavior, player, level, tick_rate)?;
        crate::inventory::check_container_menu_still_valid(player, level)?;
        super::food::tick(behavior, player, level)?;
        use crate::stats::{Stat, custom};
        award_stat(player, Stat::Custom(custom::Id::PLAY_TIME), 1, level)?;
        award_stat(player, Stat::Custom(custom::Id::TOTAL_WORLD_TIME), 1, level)?;
        if behavior.is_alive(player, level)? {
            award_stat(player, Stat::Custom(custom::Id::TIME_SINCE_DEATH), 1, level)?;
        }
        if level.entity::<EntityData>(player)?.shared_flag(1) {
            award_stat(player, Stat::Custom(custom::Id::CROUCH_TIME), 1, level)?;
        }
        if level
            .entity::<crate::entity::LivingEntity>(player)?
            .entity_data
            .get(crate::entity::synched_data::SLEEPING_POS)
            .is_none()
        {
            award_stat(player, Stat::Custom(custom::Id::TIME_SINCE_REST), 1, level)?;
        }
    }
    // Pending special inventory/map item synchronization.
    sync_health(player, level)?;
    sync_scores(player, level)?;
    super::experience::send_experience_update(player, level)?;
    // Pending LOCATION here when tickCount % 20 == 0, after the XP packet.
    // Enable once canonical equipment/stepping and required structure facts land.
    Ok(())
}

/// Port of ServerPlayer#awardStat, using the canonical stats/scoreboard adapter.
pub fn award_stat(
    player: EntityRef,
    stat: Stat,
    amount: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    crate::stats::award_stat(level, player, stat, amount)?;
    level.emit_scoreboard_packets()
}
/// Port of ServerPlayer#resetStat. Preserve the adapter's counter-first order.
pub fn reset_stat(player: EntityRef, stat: Stat, level: &mut Level) -> Result<(), LevelError> {
    crate::stats::reset_stat(level, player, stat)?;
    level.emit_scoreboard_packets()
}
fn sync_health(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let p = level.entity::<ServerPlayer>(player)?;
    let (health, food, saturation) = (
        p.health(),
        p.food_data.food_level(),
        p.food_data.saturation_level(),
    );
    if health != p.last_sent_health
        || food != p.last_sent_food
        || (saturation == 0.0) != p.last_food_saturation_zero
    {
        level.emit(LevelEffect::PlayerPacket {
            recipients: vec![player],
            packet: Box::new(cb::Packet::SetHealth(cb::SetHealth {
                health,
                food: VarInt(food),
                saturation,
            })),
        });
        let p = level.entity_mut::<ServerPlayer>(player)?;
        p.last_sent_health = p.health();
        p.last_sent_food = p.food_data.food_level();
        p.last_food_saturation_zero = p.food_data.saturation_level() == 0.0;
    }
    Ok(())
}
fn update_score_for_criteria(
    player: EntityRef,
    criterion: &str,
    value: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    let scoreboard = level.shared()?.scoreboard.scoreboard();
    let needs_display = scoreboard.objectives_for_criterion(criterion).any(|name| {
        scoreboard
            .get_objective(name)
            .is_some_and(|objective| objective.display_auto_update)
    });
    let display = needs_display
        .then(|| level.entity_display_name(player))
        .transpose()?;
    let holder = level.entity_scoreboard_name(player)?.into_owned();
    level
        .shared_mut()?
        .scoreboard
        .set_criterion_score(&holder, criterion, value, display.as_ref())
        .map_err(|_| LevelError::Inventory("player scoreboard criteria"))?;
    level.emit_scoreboard_packets()
}
/// Port of ServerPlayer#doTick's changed-value criteria, in native order. Read
/// each next value only after the preceding synchronous scoreboard callback.
fn sync_scores(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let p = level.entity_mut::<ServerPlayer>(player)?;
    let health_and_absorption = p.health() + *p.entity_data.get(data::PLAYER_ABSORPTION);
    if health_and_absorption != p.last_recorded_health_and_absorption {
        p.last_recorded_health_and_absorption = health_and_absorption;
        update_score_for_criteria(
            player,
            "health",
            mth::ceil(f64::from(health_and_absorption)),
            level,
        )?;
    }
    macro_rules! sync {
        ($field:ident, $value:expr, $criterion:literal) => {{
            let value = $value;
            let p = level.entity_mut::<ServerPlayer>(player)?;
            if value != p.$field {
                p.$field = value;
                // Native Mth.ceil(int) selects the float overload.
                update_score_for_criteria(
                    player,
                    $criterion,
                    mth::ceil(f64::from(value as f32)),
                    level,
                )?;
            }
        }};
    }
    sync!(
        last_recorded_food_level,
        level.entity::<ServerPlayer>(player)?.food_data.food_level(),
        "food"
    );
    sync!(
        last_recorded_air_level,
        level
            .entity::<EntityData>(player)?
            .entity_data
            .get(data::AIR_SUPPLY)
            .0,
        "air"
    );
    sync!(
        last_recorded_armor,
        mth::floor(
            level
                .entity::<crate::entity::LivingEntity>(player)?
                .attributes
                .value(Attribute::ARMOR)
                .expect("player supplier defines ARMOR")
        ),
        "armor"
    );
    sync!(
        last_recorded_experience,
        level.entity::<ServerPlayer>(player)?.total_experience,
        "xp"
    );
    sync!(
        last_recorded_level,
        level.entity::<ServerPlayer>(player)?.experience_level,
        "level"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
