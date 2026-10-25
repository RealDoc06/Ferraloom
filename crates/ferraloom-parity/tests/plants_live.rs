//! Live Strict plant mechanics, sharing one disposable world per server.
#[path = "plants_live/growth.rs"]
mod growth;
mod level_support;
#[path = "plants_live/soil.rs"]
mod soil;
#[path = "plants_live/support.rs"]
mod support;
#[path = "plants_live/uses.rs"]
mod uses;
#[path = "plants_live/weather.rs"]
mod weather;

use anyhow::Context;
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, WorldPreset, ensure,
    protocol::play::{BlockHitResult, InteractionHand},
};
use ferraloom_util::{BlockPos, Direction};
use level_support::{block, command, expect_blocks as wait_for_blocks, freeze, options, step};

const Y: i32 = -60;
fn at(x: i32, z: i32) -> BlockPos {
    BlockPos::new(x, Y, z)
}
fn state(block: Block, properties: &[(&'static str, &'static str)]) -> BlockState {
    BlockState::from_properties(block, properties.iter().copied()).expect("native test state")
}
fn block_argument(state: BlockState) -> String {
    let properties = state
        .properties()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}[{properties}]", state.block().name())
}
async fn expect_blocks(bot: &mut Bot, expected: &[(BlockPos, BlockState)]) -> Result<()> {
    wait_for_blocks(bot, expected).await.with_context(|| {
        let snapshot = bot.snapshot();
        let observed: Vec<_> = expected
            .iter()
            .map(|(pos, _)| (*pos, snapshot.block(*pos)))
            .collect();
        format!("expected {expected:?}; observed {observed:?}")
    })
}
async fn put(bot: &mut Bot, pos: BlockPos, state: BlockState) -> Result<()> {
    if block(bot, pos)? == state {
        return Ok(());
    }
    if state == state.block().default_state() {
        level_support::set(bot, pos, state.block()).await?;
    } else {
        command(
            bot,
            &format!(
                "setblock {} {} {} {}",
                pos.x,
                pos.y,
                pos.z,
                block_argument(state)
            ),
            "commands.setblock.success",
        )
        .await?;
    }
    expect_blocks(bot, &[(pos, state)]).await
}
async fn fill(bot: &mut Bot, from: BlockPos, to: BlockPos, state: BlockState) -> Result<()> {
    // fill.failed is native's zero-changes result, valid for arena cleanup;
    // the endpoint observations also serve as a chunk-broadcast barrier.
    command(
        bot,
        &format!(
            "fill {} {} {} {} {} {} {} strict",
            from.x,
            from.y,
            from.z,
            to.x,
            to.y,
            to.z,
            block_argument(state)
        ),
        "commands.fill.",
    )
    .await?;
    expect_blocks(bot, &[(from, state), (to, state)]).await
}
async fn random_ticks(bot: &mut Bot, speed: i32) -> Result<()> {
    // GameRuleCommand#setRule throws not_set when the requested value is
    // already installed. Startup and each arena reset deliberately request 0.
    let response = command(
        bot,
        &format!("gamerule minecraft:random_tick_speed {speed}"),
        "commands.gamerule.",
    )
    .await?;
    // CommandSourceStack#sendFailure wraps not_set in a red parent component.
    fn accepted(component: &serde_json::Value) -> bool {
        matches!(
            component
                .get("translate")
                .and_then(serde_json::Value::as_str),
            Some("commands.gamerule.set" | "commands.gamerule.not_set")
        ) || component
            .get("extra")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|extra| extra.iter().any(accepted))
    }
    ensure!(
        accepted(&response),
        "unexpected random tick rule response: {response}"
    );
    Ok(())
}
async fn reset(bot: &mut Bot) -> Result<()> {
    random_ticks(bot, 0).await?;
    bot.teleport([8.5, -60.0, 8.5]).await?;
    fill(
        bot,
        at(-8, -8),
        BlockPos::new(23, -32, 23),
        Block::AIR.default_state(),
    )
    .await?;
    fill(
        bot,
        at(-8, -8).below(1),
        at(23, 23).below(1),
        Block::STONE.default_state(),
    )
    .await?;
    step(bot, 2).await?;
    Ok(())
}
async fn until(
    bot: &mut Bot,
    description: &str,
    rounds: u32,
    ticks: i64,
    matches: impl Fn(&Bot) -> Result<bool>,
) -> Result<()> {
    for _ in 0..rounds {
        if matches(bot)? {
            return Ok(());
        }
        step(bot, ticks).await?;
    }
    ensure!(matches(bot)?, "{description}");
    Ok(())
}
fn record(log: &mut ObservationLog, bot: &Bot, label: &str, positions: &[BlockPos]) -> Result<()> {
    log.record(
        label,
        positions
            .iter()
            .map(|pos| block(bot, *pos).map(BlockState::id))
            .collect::<Result<Vec<_>>>()?,
    )
}

async fn plants(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(bot, "tick rate 1000", "commands.tick.rate.success").await?;
    command(bot, "time set noon", "commands.time.set").await?;
    command(bot, "weather clear 100000", "commands.weather.set.clear").await?;
    let mut log = ObservationLog::default();
    // Every group resets its arena. Collect independent failures so a native
    // preflight reports all fixture errors in this one server lifetime.
    let mut failures = Vec::new();
    macro_rules! group {
        ($label:literal, $run:path) => {
            if let Err(error) = $run(bot, &mut log).await {
                failures.push(format!("{}: {error:#}", $label));
                if bot.snapshot().error.is_some() {
                    anyhow::bail!("plant connection failed: {}", failures.join("\n"));
                }
            }
        };
    }
    group!("support updates", support::survival);
    group!("leaf distance and decay", support::leaves);
    group!("crop growth and light", growth::crops);
    group!("grass and mycelium", soil::spreading);
    group!("bamboo, cactus and cane", growth::columns);
    group!("farmland hydration and trampling", soil::farmland);
    group!("bone meal and saplings", uses::bonemeal);
    group!("dripleaf contact and power", uses::dripleaf);
    group!("vines and kelp", growth::vines_and_kelp);
    group!("snow and ice melting", weather::melting);
    group!("snow and ice formation", weather::precipitation);
    command(bot, "save-all flush", "commands.save.success").await?;
    ensure!(
        failures.is_empty(),
        "plant groups failed:\n{}",
        failures.join("\n")
    );
    Ok(log)
}

// Native preflight: FERRALOOM_PARITY_SIDE=vanilla cargo test -p ferraloom-parity
// --test plants_live --locked -- --ignored --nocapture. Use both for Strict parity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and released live item/block integration"]
async fn parity_live_plant_mechanics() -> Result<()> {
    let mut launch = options();
    // Clear weather and lit irrigation isolate earlier groups; the final group
    // uses this same world's real cold biome precipitation without a second boot.
    launch.preset = WorldPreset::Flat {
        layers: vec![("minecraft:stone".into(), 4)],
        biome: "minecraft:snowy_plains".into(),
    };
    launch
        .game_rules
        .insert("minecraft:block_drops".into(), "false".into());
    launch
        .properties
        .insert("spawn-protection".into(), "0".into());
    ferraloom_parity::run(launch, RunMode::from_env()?, plants).await
}
