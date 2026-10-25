#[expect(
    dead_code,
    reason = "This scenario observes packets during the stepped interval."
)]
mod level_support;

use anyhow::Context;
use ferraloom_data::{Block, EntityType, properties};
use ferraloom_parity::{
    ObservationLog, Result, RunMode, Session, Ticks, ensure, protocol::play::clientbound as cb, run,
};
use ferraloom_util::BlockPos;
use level_support::{block, command, freeze, game_time, set};
use std::time::Duration;

async fn falling_anvil(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    freeze(bot).await?;
    command(bot, "gamemode survival", "commands.gamemode.success.self").await?;
    let damage_types = bot
        .sequence()
        .registries
        .into_iter()
        .find(|(name, _)| name == "minecraft:damage_type")
        .context("damage type registry")?
        .1;
    let own_id = bot.snapshot().entity_id.context("player network id")?;
    let mut previous_health = 20.0;
    let mut log = ObservationLog::default();

    for (name, x, initial, expected, event_id) in [
        ("land", 8, Block::ANVIL, Block::CHIPPED_ANVIL, 1031),
        ("break", 12, Block::DAMAGED_ANVIL, Block::AIR, 1029),
    ] {
        let pos = BlockPos::new(x, -60, 8);
        set(bot, pos.below(1), Block::STONE).await?;
        bot.teleport([f64::from(x) + 0.5, -60.0, 8.5]).await?;
        // The native NBT damage multiplier keeps this nonlethal while a fall
        // over 20 blocks makes the real degradation probability exceed one.
        command(bot, &format!(
            "summon minecraft:falling_block {} -36 8.5 {{BlockState:{{Name:'{}',Properties:{{facing:'east'}}}},HurtEntities:1b,FallHurtAmount:0.25f,FallHurtMax:40}}",
            f64::from(x) + 0.5, initial.name()
        ), "commands.summon.success").await?;
        if !bot
            .snapshot()
            .entities
            .values()
            .any(|entity| entity.entity_type == i32::from(EntityType::FALLING_BLOCK.protocol_id()))
        {
            bot.wait_for("tracked falling anvil", Ticks::DEFAULT, |state, _| {
                state.entities.values().any(|entity| {
                    entity.entity_type == i32::from(EntityType::FALLING_BLOCK.protocol_id())
                })
            })
            .await?;
        }
        let falling_id = bot
            .snapshot()
            .entities
            .values()
            .find(|entity| entity.entity_type == i32::from(EntityType::FALLING_BLOCK.protocol_id()))
            .context("falling anvil id")?
            .id;
        let start = game_time(bot).await?;
        bot.command("tick step 80").await?;
        let mut order = Vec::new();
        let mut damage = None;
        let mut health = None;
        let mut landing_event = None;
        let mut removed = false;
        let mut update = false;
        let mut transient = false;
        bot.wait_for(
            "anvil damage, degradation and landing callbacks",
            Ticks(120),
            |_, packet| {
                match packet {
                    cb::Packet::DamageEvent(event) if event.entity_id.0 == own_id => {
                        order.push("damage");
                        damage = Some((
                            event.source_type.0,
                            event.source_cause_id.0.map(|id| id == falling_id),
                            event.source_direct_id.0.map(|id| id == falling_id),
                        ));
                    }
                    cb::Packet::BlockUpdate(event) if event.pos == pos && !update => {
                        update = true;
                        order.push("block_update");
                    }
                    cb::Packet::AddTransientBlock(event) if event.pos == pos => {
                        transient = true;
                        order.push("transient_block");
                    }
                    cb::Packet::RemoveEntities(event)
                        if event.entity_ids.iter().any(|id| id.0 == falling_id) =>
                    {
                        removed = true;
                        order.push("remove");
                    }
                    cb::Packet::LevelEvent(event)
                        if event.pos == pos && [1029, 1031].contains(&event.event_type) =>
                    {
                        order.push("landing_event");
                        landing_event = Some(event.event_type);
                    }
                    cb::Packet::SetHealth(event) if event.health < previous_health => {
                        order.push("health");
                        health = Some(event.health);
                    }
                    _ => {}
                }
                damage.is_some() && health.is_some() && landing_event.is_some() && removed
            },
        )
        .await?;
        let health = health.context("anvil health update")?;
        ensure!(
            health > 0.0 && health < previous_health,
            "nonlethal anvil damage: {previous_health} -> {health}"
        );
        previous_health = health;
        let damage = damage.context("anvil DamageEvent")?;
        let source_name = damage_types
            .get(usize::try_from(damage.0)?)
            .context("anvil damage type")?;
        ensure!(
            source_name == "minecraft:falling_anvil"
                && damage.1 == Some(true)
                && damage.2 == Some(true),
            "anvil source: {source_name}, {damage:?}"
        );
        ensure!(
            landing_event == Some(event_id),
            "anvil callback: {landing_event:?}"
        );
        let index = |name| {
            order
                .iter()
                .position(|entry| *entry == name)
                .context("required ordered anvil output")
        };
        ensure!(
            index("damage")? < index("remove")? && index("remove")? < index("landing_event")?,
            "damage/remove/callback order: {order:?}"
        );
        if expected != Block::AIR {
            ensure!(
                update
                    && transient
                    && index("block_update")? < index("transient_block")?
                    && index("transient_block")? < index("remove")?,
                "placement packet order: {order:?}"
            );
        } else {
            ensure!(!transient, "broken anvil sent a placement packet");
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let now = game_time(bot).await?;
                ensure!(now <= start + 80, "anvil step overshot");
                if now == start + 80 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        bot.synchronize().await?;
        let expected = if expected == Block::AIR {
            expected.default_state()
        } else {
            expected
                .default_state()
                .with(properties::FACING, properties::Facing::East)
                .context("anvil facing property")?
        };
        ensure!(
            block(bot, pos)? == expected,
            "anvil did not degrade to {expected:?}"
        );
        log.record(format!("anvil.{name}.packet_order"), order)?;
        log.record(format!("anvil.{name}.damage_type"), source_name)?;
        log.record(format!("anvil.{name}.health_bits"), health.to_bits())?;
        log.record(format!("anvil.{name}.block"), expected.id())?;
        log.record(format!("anvil.{name}.callback"), landing_event)?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both Strict servers, falling-block hooks and canonical damage/tracker integration"]
async fn parity_anvils_damage_degrade_and_emit_native_landing_or_break_events() -> Result<()> {
    let mut options = level_support::options();
    options.game_rules.insert(
        "minecraft:natural_health_regeneration".into(),
        "false".into(),
    );
    run(options, RunMode::from_env()?, falling_anvil).await
}
