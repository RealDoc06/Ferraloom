mod level_support;

use anyhow::Context;
use ferraloom_data::registries::{custom_stat, stat_type};
use ferraloom_parity::{
    ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::play::{self, clientbound as cb, serverbound as sb},
    run,
};
use level_support::{command, freeze};

async fn survival_fall(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    freeze(bot).await?;
    if bot.snapshot().game_mode != Some(play::GameType::Survival) {
        command(bot, "gamemode survival", "commands.gamemode.success.self").await?;
    }
    bot.teleport([8.5, -54.0, 8.5]).await?;
    let teleports = bot.snapshot().teleports;
    bot.move_to([8.5, -59.0, 8.5], false).await?;
    let damage_types = bot
        .sequence()
        .registries
        .into_iter()
        .find(|(name, _)| name == "minecraft:damage_type")
        .context("damage type registry missing")?
        .1;
    let own_id = bot.snapshot().entity_id.context("player network id")?;
    let rotation = bot.snapshot().rotation;
    // Observe the final move directly: move_to's synchronization barrier would
    // consume the damage and health packets before this ordered observation.
    bot.send(sb::Packet::MovePlayerPosRot(sb::MovePlayerPosRot {
        x: 8.5,
        y: -60.0,
        z: 8.5,
        y_rot: rotation[0],
        x_rot: rotation[1],
        flags: play::MovementFlags {
            on_ground: true,
            horizontal_collision: false,
        },
    }))
    .await?;
    bot.send(sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
        .await?;
    let mut order = Vec::new();
    let mut damage_source = None;
    let mut hurt_yaw = None;
    let mut health = None;
    bot.wait_for(
        "nonlethal fall packets and health",
        Ticks::DEFAULT,
        |_, packet| {
            match packet {
                cb::Packet::DamageEvent(event) if event.entity_id.0 == own_id => {
                    order.push("damage");
                    damage_source = Some((
                        event.source_type.0,
                        event.source_cause_id.0,
                        event.source_direct_id.0,
                        event.source_position.is_some(),
                    ));
                }
                cb::Packet::HurtAnimation(event) if event.id.0 == own_id => {
                    order.push("hurt_animation");
                    hurt_yaw = Some(event.yaw.to_bits());
                }
                cb::Packet::SetHealth(update) if update.health < 20.0 => {
                    order.push("health");
                    health = Some((
                        update.health.to_bits(),
                        update.food.0,
                        update.saturation.to_bits(),
                    ));
                    return true;
                }
                _ => {}
            }
            false
        },
    )
    .await?;
    ensure!(
        order == ["damage", "hurt_animation", "health"],
        "fall packet order: {order:?}"
    );
    let (source_id, cause, direct, position) = damage_source.context("fall damage event")?;
    let source_name = damage_types
        .get(usize::try_from(source_id)?)
        .context("unknown fall damage registry id")?;
    ensure!(
        source_name == "minecraft:fall" && cause.is_none() && direct.is_none() && !position,
        "unexpected fall source: {source_name}, {cause:?}, {direct:?}, {position}"
    );
    let health = health.context("fall health update")?;
    ensure!(
        f32::from_bits(health.0) == 17.0,
        "six-block fall health: {health:?}"
    );
    ensure!(
        bot.snapshot().teleports == teleports,
        "server corrected the valid falling path"
    );

    bot.send(sb::Packet::ClientCommand(sb::ClientCommand {
        action: play::ClientCommandAction::RequestStats,
    }))
    .await?;
    let stats = bot
        .wait_for("fall statistics", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::AwardStats(_))
        })
        .await?;
    let cb::Packet::AwardStats(stats) = stats.as_ref() else {
        unreachable!()
    };
    let value = |stat: custom_stat::Id| {
        stats
            .stats
            .iter()
            .find(|entry| {
                entry.stat_type.0 == i32::from(stat_type::Id::CUSTOM.protocol_id())
                    && entry.value.0 == i32::from(stat.protocol_id())
            })
            .map_or(0, |entry| entry.count.0)
    };
    let taken = value(custom_stat::Id::DAMAGE_TAKEN);
    let absorbed = value(custom_stat::Id::DAMAGE_ABSORBED);
    ensure!(
        taken == 30 && absorbed == 0,
        "fall damage stats: taken={taken}, absorbed={absorbed}"
    );

    let mut log = ObservationLog::default();
    log.record("fall.packet_order", order)?;
    log.record("fall.source", source_name)?;
    log.record(
        "fall.hurt_yaw_bits",
        hurt_yaw.context("hurt animation yaw")?,
    )?;
    log.record("fall.health_food_saturation_bits", health)?;
    log.record("fall.damage_stats", [taken, absorbed])?;
    session.stop().await?;
    let saved = session.saved_world()?.player("ParityBot")?;
    let saved_health = saved.get_float("Health").context("saved Health")?;
    let saved_absorption = saved
        .get_float("AbsorptionAmount")
        .context("saved AbsorptionAmount")?;
    ensure!(
        saved_health == 17.0 && saved_absorption == 0.0,
        "saved health/absorption: {saved_health}/{saved_absorption}"
    );
    log.record(
        "fall.saved_health_absorption_bits",
        [saved_health.to_bits(), saved_absorption.to_bits()],
    )?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both Strict servers and canonical damage/tracker integration"]
async fn parity_survival_fall_preserves_damage_packets_stats_health_and_absorption() -> Result<()> {
    let mut options = level_support::options();
    options.game_rules.insert(
        "minecraft:natural_health_regeneration".into(),
        "false".into(),
    );
    run(options, RunMode::from_env()?, survival_fall).await
}
