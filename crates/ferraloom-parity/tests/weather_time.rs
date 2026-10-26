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
async fn stepped_window(bot: &mut Bot, ticks: i64) -> Result<()> {
    let initial = level_support::game_time(bot).await?;
    bot.command(&format!("tick step {ticks}")).await?;
    let target = initial + ticks;
    let mut failure = None;
    bot.wait_for(
        "stepped game-time synchronization",
        Ticks::DEFAULT,
        |_, packet| {
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
    Ok(())
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
