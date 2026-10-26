//! Live ServerLevel#tickTime and ServerClockManager packet observations.
#[expect(
    dead_code,
    reason = "This scenario uses only the shared command/world setup helpers."
)]
mod level_support;

use ferraloom_parity::protocol::play::clientbound as cb;
use ferraloom_parity::{Bot, ObservationLog, Result, RunMode, Session, Ticks, ensure, run};
use serde_json::json;

/// ADVANCE_TIME sends the complete native clock map. Its identity-keyed map
/// order differs in the two JVM captures in commands/vanilla/full-clock-map-orders.json
/// (tests/java/commands/run-clock-map-order.sh). Compare only keys by registry id,
/// retaining every field, as the command packet harness does.
async fn advance_time(bot: &mut Bot, enabled: bool) -> Result<serde_json::Value> {
    bot.command(&format!("gamerule advance_time {enabled}"))
        .await?;
    let mut clocks = None;
    let mut feedback = false;
    let mut failure = None;
    bot.wait_for(
        "advance_time feedback and full clock synchronization",
        Ticks::DEFAULT,
        |_, packet| {
            match packet {
                cb::Packet::SetTime(packet) if !packet.clock_updates.is_empty() => {
                    let mut values = packet
                        .clock_updates
                        .iter()
                        .map(|update| {
                            (
                                update.clock.0,
                                update.state.total_ticks.0,
                                update.state.partial_tick,
                                update.state.rate,
                            )
                        })
                        .collect::<Vec<_>>();
                    values.sort_by_key(|value| value.0);
                    clocks = Some(json!(values));
                }
                cb::Packet::SystemChat(packet) => {
                    if let Ok(text) = packet.content.to_json() {
                        feedback |= text.contains("commands.gamerule.set");
                        if text.contains("commands.generic.failed")
                            || text.contains("ferraloom.command.pending")
                        {
                            failure = Some(text);
                        }
                    }
                }
                _ => {}
            }
            failure.is_some() || (feedback && clocks.is_some())
        },
    )
    .await?;
    ensure!(failure.is_none(), "advance_time failed: {failure:?}");
    Ok(clocks.expect("both the feedback and clock packet were received"))
}

/// Frozen ticks still send periodic game-time packets. Wait for the stepped
/// window's final value rather than counting repeated frozen broadcasts, whose
/// arrival depends on the network and is not simulated time.
async fn stepped_window(bot: &mut Bot, ticks: i64) -> Result<Vec<(u8, u32)>> {
    let initial = level_support::game_time(bot).await?;
    bot.command(&format!("tick step {ticks}")).await?;
    let target = initial + ticks;
    let mut failure = None;
    let mut weather = Vec::new();
    bot.wait_for(
        "stepped game-time synchronization",
        Ticks::DEFAULT,
        |_, packet| {
            if let cb::Packet::GameEvent(event) = packet
                && matches!(event.event, 1 | 2 | 7 | 8)
            {
                weather.push((event.event, event.param.to_bits()));
            }
            if let cb::Packet::SetTime(packet) = packet {
                if !packet.clock_updates.is_empty()
                    || packet.game_time < initial
                    || packet.game_time > target
                {
                    failure = Some(format!("unexpected periodic time packet: {packet:?}"));
                    return true;
                }
                return packet.game_time == target;
            }
            false
        },
    )
    .await?;
    ensure!(failure.is_none(), "{failure:?}");
    ensure!(
        level_support::game_time(bot).await? == target,
        "tickTime must advance independently of advance_time"
    );
    Ok(weather)
}

async fn clocks(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    level_support::freeze(bot).await?;
    advance_time(bot, false).await?;
    for clock in ["minecraft:overworld", "minecraft:the_end"] {
        level_support::command(
            bot,
            &format!("time of {clock} set 1000"),
            "commands.time.set.absolute",
        )
        .await?;
    }
    let mut log = ObservationLog::default();
    for (index, enabled, total) in [
        (0, true, 1000),
        (1, false, 1040),
        (2, true, 1040),
        (3, false, 1080),
    ] {
        if index != 0 {
            stepped_window(bot, 40).await?;
        }
        let values = advance_time(bot, enabled).await?;
        let values = values.as_array().expect("clock map array");
        ensure!(values.len() == 2, "both built-in clocks must synchronize");
        for value in values {
            ensure!(
                value[1] == total && value[2] == 0.0 && value[3] == if enabled { 1.0 } else { 0.0 },
                "unexpected clock state: {value}"
            );
        }
        log.record(format!("window.{index}"), values)?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK; Strict time packet comparison"]
async fn stepped_time_and_daylight_rule_packets_match_vanilla() -> Result<()> {
    run(level_support::options(), RunMode::from_env()?, clocks).await
}

/// Native weather durations keep counting only with ADVANCE_WEATHER; the
/// per-level 0.01F interpolation keeps running while the rule is disabled.
async fn weather(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    level_support::freeze(bot).await?;
    // LaunchOptions already installs advance_weather=false. Native rejects
    // assigning an unchanged rule, so do not wait for a set-success response.
    level_support::command(bot, "weather clear 1000", "commands.weather.set.clear").await?;
    stepped_window(bot, 110).await?;
    let mut log = ObservationLog::default();
    for (label, command, key, ticks) in [
        (
            "rain.rule-disabled",
            "weather rain 20t",
            "commands.weather.set.rain",
            25,
        ),
        (
            "thunder.rule-disabled",
            "weather thunder 1000",
            "commands.weather.set.thunder",
            110,
        ),
        (
            "clear.rule-disabled",
            "weather clear 1000",
            "commands.weather.set.clear",
            110,
        ),
        (
            "rain.timer-prepared",
            "weather rain 20t",
            "commands.weather.set.rain",
            25,
        ),
    ] {
        level_support::command(bot, command, key).await?;
        let events = stepped_window(bot, ticks).await?;
        ensure!(!events.is_empty(), "{label} must interpolate weather");
        log.record(label, events)?;
    }
    level_support::command(
        bot,
        "gamerule advance_weather true",
        "commands.gamerule.set",
    )
    .await?;
    log.record("rain.timer-expiry", stepped_window(bot, 45).await?)?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK; Strict weather packet comparison"]
async fn stepped_weather_levels_transitions_and_rule_match_vanilla() -> Result<()> {
    run(level_support::options(), RunMode::from_env()?, weather).await
}

/// Observe converged block states, not which random precipitation column won
/// first. The biome, light and full-chunk queries are the real live paths.
async fn precipitation(session: &mut Session) -> Result<ObservationLog> {
    use ferraloom_data::{Block, properties};
    use ferraloom_util::BlockPos;
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    level_support::freeze(bot).await?;
    bot.teleport([8.5, -60.0, 12.5]).await?;
    let water = BlockPos::new(6, -59, 8);
    let snow = BlockPos::new(10, -59, 8);
    for (text, key) in [
        // Four flat layers end at -61; give the -59 snow a real -60 support.
        (
            "fill 5 -60 7 11 -60 9 minecraft:stone",
            "commands.fill.success",
        ),
        (
            "fill 5 -59 7 7 -59 9 minecraft:stone",
            "commands.fill.success",
        ),
        (
            "setblock 6 -59 8 minecraft:water",
            "commands.setblock.success",
        ),
        (
            "setblock 10 -59 8 minecraft:snow[layers=1]",
            "commands.setblock.success",
        ),
        (
            "gamerule max_snow_accumulation_height 2",
            "commands.gamerule.set",
        ),
        ("weather rain 100000t", "commands.weather.set.rain"),
        ("gamerule random_tick_speed 1024", "commands.gamerule.set"),
    ] {
        level_support::command(bot, text, key).await?;
    }
    level_support::expect_blocks(
        bot,
        &[
            (water, Block::WATER.default_state()),
            (snow, Block::SNOW.default_state()),
        ],
    )
    .await?;
    for _ in 0..20 {
        level_support::step(bot, 20).await?;
        let snapshot = bot.snapshot();
        if snapshot
            .block(water)
            .is_some_and(|state| state.block() == Block::ICE)
            && snapshot.block(snow).is_some_and(|state| {
                state.block() == Block::SNOW && state.get(properties::LAYERS) == Some(2)
            })
        {
            break;
        }
    }
    ensure!(
        level_support::block(bot, water)?.block() == Block::ICE,
        "source water did not freeze: {:?}",
        level_support::block(bot, water)?
    );
    ensure!(
        level_support::block(bot, snow)?
            == Block::SNOW
                .default_state()
                .with(properties::LAYERS, 2)
                .unwrap(),
        "snow did not reach its two-layer cap: {:?}",
        level_support::block(bot, snow)?
    );
    level_support::step(bot, 40).await?;
    let mut log = ObservationLog::default();
    for (label, pos) in [
        ("freeze.edge_source", water),
        ("snow.rule_capped_layers", snow),
    ] {
        log.record(label, level_support::block(bot, pos)?.id())?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK; Strict cold precipitation comparison"]
async fn cold_precipitation_freezes_water_and_caps_snow_layers() -> Result<()> {
    let mut options = level_support::options();
    options.preset = ferraloom_parity::WorldPreset::Flat {
        layers: vec![("minecraft:stone".into(), 4)],
        biome: "minecraft:snowy_plains".into(),
    };
    run(options, RunMode::from_env()?, precipitation).await
}
