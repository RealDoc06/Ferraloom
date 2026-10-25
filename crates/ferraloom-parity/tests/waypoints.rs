//! Native waypoint packets from consumed join, movement, team, rule and leave paths.
#[expect(
    dead_code,
    reason = "Waypoints use the non-block helpers and keep world ticks frozen."
)]
mod level_support;
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        self as p, Uuid, Wire,
        play::{self, clientbound as cb},
    },
};

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        properties: [("gamemode".into(), "creative".into())].into(),
        operators: vec!["ParityBot".into(), "WaypointOther".into()],
        ..level_support::options()
    }
}
fn wire(packet: &impl Wire) -> Result<String> {
    Ok(packet
        .to_bytes()?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn translation(packet: &cb::Packet, key: &str) -> bool {
    matches!(packet, cb::Packet::SystemChat(message) if message.content.to_json().is_ok_and(|json|
        serde_json::from_str::<serde_json::Value>(&json).is_ok_and(|value| value["translate"] == key)))
}
/// The seed response fences the command queue; PingResponse alone runs on the
/// vanilla network thread and cannot fence a command's synchronous callbacks.
async fn command(bot: &mut Bot, text: &str) -> Result<Vec<String>> {
    bot.command(text).await?;
    bot.command("seed").await?;
    let mut packets = Vec::new();
    let mut error = None;
    bot.wait_for(text, Ticks::DEFAULT, |_, packet| {
        if translation(packet, "commands.seed.success") {
            return true;
        }
        match packet {
            cb::Packet::Waypoint(packet) => {
                packets.push(format!("waypoint:{}", wire(packet).unwrap()))
            }
            cb::Packet::SetPlayerTeam(packet) => {
                packets.push(format!("team:{}", wire(packet).unwrap()))
            }
            cb::Packet::SystemChat(message) => {
                let json = message.content.to_json().unwrap();
                if json.contains("ferraloom.") || translation(packet, "commands.generic.failed") {
                    error = Some(json);
                    return true;
                }
                if translation(packet, "commands.gamerule.set") {
                    packets.push("gamerule.feedback".into());
                }
            }
            _ => {}
        }
        false
    })
    .await?;
    ensure!(error.is_none(), "{text}: {error:?}");
    Ok(packets)
}
fn waypoint(bot: &Bot, uuid: Uuid) -> Option<play::TrackedWaypointData> {
    bot.snapshot()
        .waypoints
        .values()
        .find(|waypoint| waypoint.identifier == p::codec::Either::Left(uuid))
        .cloned()
}
async fn wait_waypoint(bot: &mut Bot, uuid: Uuid) -> Result<play::TrackedWaypointData> {
    if waypoint(bot, uuid).is_none() {
        bot.wait_for("live waypoint transmitter", Ticks::DEFAULT, |state, _| {
            state
                .waypoints
                .values()
                .any(|waypoint| waypoint.identifier == p::codec::Either::Left(uuid))
        })
        .await?;
    }
    waypoint(bot, uuid).ok_or_else(|| anyhow::anyhow!("missing waypoint after observation"))
}
async fn setup(
    session: &mut Session,
) -> Result<(
    ferraloom_parity::BotId,
    ferraloom_parity::BotId,
    ObservationLog,
)> {
    let first = session.join("ParityBot").await?;
    level_support::freeze(session.bot(first)?).await?;
    session.bot(first)?.teleport([8.5, 80.0, 10.5]).await?;
    let second = session.join("WaypointOther").await?;
    session.bot(second)?.synchronize().await?;
    let initial = wait_waypoint(session.bot(second)?, Uuid::offline_player("ParityBot")).await?;
    ensure!(
        initial.icon.style.as_str() == "minecraft:default" && initial.icon.color.is_none(),
        "wrong initial waypoint icon"
    );
    let mut log = ObservationLog::default();
    log.record(
        "join.retained_transmitter",
        initial.identifier == p::codec::Either::Left(Uuid::offline_player("ParityBot")),
    )?;
    session.bot(second)?.teleport([12.5, 80.0, 10.5]).await?;
    // Chunk delivery and the positional acknowledgment both precede this state read.
    session.bot(first)?.synchronize().await?;
    wait_waypoint(session.bot(first)?, Uuid::offline_player("WaypointOther")).await?;
    Ok((first, second, log))
}
async fn loaded_area(session: &mut Session) -> Result<ObservationLog> {
    let (first, second, mut log) = setup(session).await?;
    let source = Uuid::offline_player("WaypointOther");
    // All positions stay inside the initial loaded chunk. This measures the
    // actual position producer without depending on new structure-data access.
    for (label, text) in [
        ("movement.update", "tp WaypointOther 13.5 80 10.5"),
        ("movement.replace", "tp WaypointOther 15.5 80 10.5"),
        ("team.create", "team add waypoint"),
        ("team.black", "team modify waypoint color black"),
        ("team.join", "team join waypoint WaypointOther"),
        ("team.second", "team add waypoint_second"),
        ("team.red", "team modify waypoint_second color red"),
        (
            "team.move_member",
            "team join waypoint_second WaypointOther",
        ),
        ("rule.disable", "gamerule minecraft:locator_bar false"),
        ("rule.enable", "gamerule minecraft:locator_bar true"),
        ("team.leave", "team leave WaypointOther"),
    ] {
        log.record(label, command(session.bot(first)?, text).await?)?;
    }
    let current = wait_waypoint(session.bot(first)?, source).await?;
    ensure!(
        matches!(current.contents, play::WaypointContents::Vec3i { ref position } if position.x.0 == 15 && position.y.0 == 80 && position.z.0 == 10),
        "wrong final waypoint position: {:?}",
        current.contents
    );
    ensure!(
        current.icon.color.is_none(),
        "team removal left the old icon color"
    );
    session.bot(second)?.disconnect().await?;
    let mut leave = Vec::new();
    session
        .bot(first)?
        .wait_for(
            "leave untracks waypoint before player-info removal",
            Ticks::DEFAULT,
            |_, packet| {
                match packet {
                    cb::Packet::RemoveEntities(_) => leave.push("RemoveEntities"),
                    cb::Packet::Waypoint(packet)
                        if packet.waypoint.identifier == p::codec::Either::Left(source) =>
                    {
                        if packet.operation == play::WaypointOperation::Untrack {
                            leave.push("Untrack");
                        }
                    }
                    cb::Packet::PlayerInfoRemove(packet)
                        if packet.profile_ids.contains(&source) =>
                    {
                        leave.push("PlayerInfoRemove");
                        return true;
                    }
                    _ => {}
                }
                false
            },
        )
        .await?;
    ensure!(
        leave == ["RemoveEntities", "Untrack", "PlayerInfoRemove"],
        "wrong waypoint leave order: {leave:?}"
    );
    ensure!(
        waypoint(session.bot(first)?, source).is_none(),
        "departed transmitter still tracked"
    );
    log.record("leave.order", leave)?;
    Ok(log)
}
/// Follow-up live leg; current Strict LOCATION needs pipeline's structure-data
/// bridge while teleporting into these freshly ticket-eligible areas.
async fn far_area(session: &mut Session) -> Result<ObservationLog> {
    let (first, _, mut log) = setup(session).await?;
    for (label, text) in [
        ("chunk.track", "tp WaypointOther 100.5 80 10.5"),
        ("chunk.update", "tp WaypointOther 116.5 80 10.5"),
        ("azimuth.track", "tp WaypointOther 380.5 80 10.5"),
        ("azimuth.update", "tp WaypointOther 380.5 80 30.5"),
    ] {
        log.record(label, command(session.bot(first)?, text).await?)?;
    }
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both disposable Strict servers on the builder"]
async fn waypoint_connections_follow_join_movement_teams_rules_and_leave() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::from_env()?, loaded_area).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "awaiting structure-data bridge for Strict fresh-area LOCATION"]
async fn waypoint_chunk_and_azimuth_connections_follow_far_movement() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::from_env()?, far_area).await
}
