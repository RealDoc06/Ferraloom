//! Optional latency measurements. Durations are printed, never parity observations
//! or assertions; the production generation path and bot deadlines stay unchanged.
use anyhow::Context;
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::play::clientbound as cb,
};
use ferraloom_util::ChunkPos;
use serde_json::Value;
use std::time::Instant;

fn has_translation(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("translate").and_then(Value::as_str) == Some(key)
                || fields.values().any(|value| has_translation(value, key))
        }
        Value::Array(values) => values.iter().any(|value| has_translation(value, key)),
        _ => false,
    }
}

async fn command(bot: &mut Bot, text: &str, key: &str) -> Result<()> {
    let start = Instant::now();
    let result = async {
        bot.command(text).await?;
        let packet = bot
            .wait_for(key, Ticks::DEFAULT, |_, packet| {
                matches!(packet, cb::Packet::SystemChat(packet)
                if packet.content.to_json().ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .is_some_and(|value| has_translation(&value, key)))
            })
            .await?;
        let cb::Packet::SystemChat(packet) = packet.as_ref() else {
            unreachable!()
        };
        ensure!(!packet.overlay, "command feedback must use chat");
        Ok(())
    }
    .await;
    eprintln!(
        "WORLDGEN_COMMAND_INTERVAL {}",
        serde_json::json!({
            "command": text,
            "elapsed_ms": start.elapsed().as_secs_f64() * 1000.,
            "outcome": if result.is_ok() { "complete" } else { "error" }
        })
    );
    result
}

async fn force_load(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    for (text, key) in [
        ("forceload query", "commands.forceload.added.none"),
        ("forceload add 1024 1024", "commands.forceload.added.single"),
        (
            "forceload add 1024 1024",
            "commands.forceload.added.failure",
        ),
        (
            "forceload query 1024 1024",
            "commands.forceload.query.success",
        ),
    ] {
        command(session.bot(id)?, text, key).await?;
    }
    session.stop().await?;
    let chunk = session
        .saved_world()?
        .chunk("minecraft:overworld", ChunkPos::new(64, 64))?
        .context("force-loaded center was not saved")?;
    ensure!(
        chunk
            .status
            .as_ref()
            .is_some_and(|status| status.to_string_lossy() == "minecraft:full"),
        "force-loaded center did not reach FULL"
    );
    Ok(ObservationLog::default())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "optional builder timing run; prints intervals and asserts correctness only"]
async fn serial_force_load_command_intervals() -> Result<()> {
    // Same controls as command_support::options and the beginning of
    // parity_command_force_load_and_weather, including Ticks::DEFAULT.
    let options = LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        ..LaunchOptions::default()
    };
    let sides = match RunMode::from_env()? {
        RunMode::Both => vec![
            ("vanilla", RunMode::Vanilla),
            ("ferraloom", RunMode::Ferraloom),
        ],
        RunMode::Vanilla => vec![("vanilla", RunMode::Vanilla)],
        RunMode::Ferraloom => vec![("ferraloom", RunMode::Ferraloom)],
    };
    for (side, mode) in sides {
        eprintln!(
            "WORLDGEN_TIMING_RUN side={side} seed={} preset=bedrock-flat target=64,64 regions=off",
            options.seed
        );
        ferraloom_parity::run(options.clone(), mode, force_load).await?;
    }
    Ok(())
}
