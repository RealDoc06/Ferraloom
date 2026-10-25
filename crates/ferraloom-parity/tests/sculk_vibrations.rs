//! Real BlockItem placement -> synchronous section listener -> vibration ticker.
mod level_support;
use ferraloom_data::{Block, properties as p};
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, ensure,
    protocol::play::{BlockHitResult, InteractionHand},
};
use ferraloom_util::{BlockPos, Direction};
use level_support::{block, command, expect_blocks, freeze, options, set, step};

const SENSOR: BlockPos = BlockPos::new(8, -60, 8);
const CALIBRATED: BlockPos = BlockPos::new(14, -60, 8);
const PLACEMENT: BlockPos = BlockPos::new(10, -60, 8);
async fn place(bot: &mut Bot) -> Result<()> {
    bot.use_item_on(
        InteractionHand::MainHand,
        BlockHitResult {
            block_pos: PLACEMENT.below(1),
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 1.0,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await?;
    expect_blocks(bot, &[(PLACEMENT, Block::STONE.default_state())]).await
}
fn assert_phase(bot: &Bot, pos: BlockPos, phase: p::SculkSensorPhase, power: u8) -> Result<()> {
    let actual = block(bot, pos)?;
    ensure!(
        actual.get(p::SCULK_SENSOR_PHASE) == Some(phase) && actual.get(p::POWER) == Some(power),
        "sensor {pos:?}: {actual:?}"
    );
    Ok(())
}
async fn vibrations(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(bot, "gamemode creative", "commands.gamemode.success").await?;
    command(
        bot,
        "forceload add -16 -16 31 31",
        "commands.forceload.added.multiple",
    )
    .await?;
    bot.teleport([8.5, -60.0, 11.5]).await?;
    bot.move_to([8.5, -60.0, 11.5], true).await?;
    step(bot, 2).await?;
    set(bot, SENSOR, Block::SCULK_SENSOR).await?;
    set(bot, CALIBRATED, Block::CALIBRATED_SCULK_SENSOR).await?;
    command(
        bot,
        "give ParityBot minecraft:stone 8",
        "commands.give.success.single",
    )
    .await?;
    place(bot).await?;
    step(bot, 4).await?;
    assert_phase(bot, SENSOR, p::SculkSensorPhase::Active, 12)?;
    assert_phase(bot, CALIBRATED, p::SculkSensorPhase::Active, 12)?;
    let mut log = ObservationLog::default();
    log.record(
        "real_block_place_activates_sensor",
        block(bot, SENSOR)?.id(),
    )?;
    log.record("calibrated_sensor_radius", block(bot, CALIBRATED)?.id())?;
    step(bot, 10).await?;
    assert_phase(bot, CALIBRATED, p::SculkSensorPhase::Cooldown, 0)?;
    assert_phase(bot, SENSOR, p::SculkSensorPhase::Active, 12)?;
    log.record("calibrated_active_duration", block(bot, CALIBRATED)?.id())?;
    step(bot, 10).await?;
    assert_phase(bot, CALIBRATED, p::SculkSensorPhase::Inactive, 0)?;
    step(bot, 8).await?;
    assert_phase(bot, SENSOR, p::SculkSensorPhase::Cooldown, 0)?;
    log.record("ordinary_active_duration", block(bot, SENSOR)?.id())?;
    step(bot, 10).await?;
    assert_phase(bot, SENSOR, p::SculkSensorPhase::Inactive, 0)?;
    log.record("cooldown_returns_inactive", block(bot, SENSOR)?.id())?;
    set(bot, CALIBRATED, Block::AIR).await?;
    set(bot, SENSOR.relative(Direction::East, 1), Block::WHITE_WOOL).await?;
    set(bot, PLACEMENT, Block::AIR).await?;
    place(bot).await?;
    step(bot, 8).await?;
    assert_phase(bot, SENSOR, p::SculkSensorPhase::Inactive, 0)?;
    log.record("wool_occludes_real_block_place", block(bot, SENSOR)?.id())?;
    command(bot, "save-all flush", "commands.save.success").await?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both native and Ferraloom live servers"]
async fn strict_sculk_sensors_receive_real_block_events() -> Result<()> {
    let mut launch = options();
    launch
        .properties
        .insert("spawn-protection".into(), "0".into());
    ferraloom_parity::run(launch, RunMode::from_env()?, vibrations).await
}
