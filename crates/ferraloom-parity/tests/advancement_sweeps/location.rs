//! ServerPlayer#doTick's LOCATION criterion, using actual equipment and supporting blocks.
use super::*;
use ferraloom_util::{BlockPos, Vec3};

const ADVANCEMENT: &str = "minecraft:adventure/walk_on_powder_snow_with_leather_boots";

fn target() -> Result<Case> {
    let resources = resources()?;
    let (_, definition) = resources
        .registry("advancement")
        .into_iter()
        .find(|(id, _)| id.to_string() == ADVANCEMENT)
        .context("native powder-snow advancement")?;
    let definition = definition.json()?;
    ensure!(
        definition["criteria"]["walk_on_powder_snow_with_leather_boots"]["trigger"]
            == "minecraft:location",
        "native powder-snow trigger changed"
    );
    case(ADVANCEMENT.into(), &definition, Vec::new())
}

async fn movement(bot: &Bot, position: Vec3) -> Result<()> {
    let rotation = bot.snapshot().rotation;
    bot.send(sb::Packet::MovePlayerPosRot(sb::MovePlayerPosRot {
        x: position.x,
        y: position.y,
        z: position.z,
        y_rot: rotation[0],
        x_rot: rotation[1],
        flags: play::MovementFlags {
            on_ground: true,
            horizontal_collision: false,
        },
    }))
    .await?;
    bot.send(sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
        .await
}

async fn incomplete_after_location_ticks(
    bot: &mut Bot,
    capture: &mut Capture<'_>,
    log: &mut ObservationLog,
    label: &str,
) -> Result<()> {
    let time = bot.snapshot().game_time.context("joined game time")?;
    let context = bot.registry_context()?;
    // Two successive time boundaries include at least one 20-tick LOCATION
    // evaluation after the input, independently of the player's tick offset.
    bot.wait_for("negative LOCATION flush", Ticks::DEFAULT, |_, packet| {
        capture.observe(packet, context.as_ref());
        matches!(packet, cb::Packet::SetTime(packet) if packet.game_time > time + 20)
    })
    .await?;
    ensure!(
        !capture.done(),
        "powder-snow advancement awarded on {label}"
    );
    log.record(label, false)
}

async fn sweep(session: &mut Session, case: &Case) -> Result<ObservationLog> {
    let player = session.join(&name("Loc", 0)).await?;
    let bot = session.bot(player)?;
    initialize(bot).await?;
    for (pos, state) in [
        (BlockPos::new(8, 79, 8), "minecraft:stone"),
        (BlockPos::new(8, 78, 9), "minecraft:stone"),
        (BlockPos::new(8, 79, 9), "minecraft:powder_snow"),
    ] {
        command(
            bot,
            &format!("setblock {} {} {} {state}", pos.x, pos.y, pos.z),
            "commands.setblock.success",
        )
        .await?;
    }
    bot.teleport([8.5, 80.0, 8.5]).await?;
    let teleports = bot.snapshot().teleports;
    let mut capture = Capture::new(case, bot)?;
    let mut log = ObservationLog::default();
    movement(bot, Vec3::new(8.5, 80.0, 8.5)).await?;
    incomplete_after_location_ticks(bot, &mut capture, &mut log, "stone.without_boots").await?;

    // InventoryMenu slot 8 is the canonical feet equipment receiver.
    set_item(bot, 8, Item::LEATHER_BOOTS, 1).await?;
    movement(bot, Vec3::new(8.5, 80.0, 8.5)).await?;
    incomplete_after_location_ticks(bot, &mut capture, &mut log, "stone.with_boots").await?;

    // PowderSnowBlock#canEntityWalkOnPowderSnow makes this adjacent block a
    // real supporting collision shape; the movement packet supplies onGround.
    movement(bot, Vec3::new(8.5, 80.0, 9.5)).await?;
    flush(bot, &mut capture, false).await?;
    ensure!(
        bot.snapshot().teleports == teleports,
        "server corrected the supported powder-snow movement"
    );
    capture.record(&mut log)?;
    session.stop().await?;
    saved(session, "Loc", std::slice::from_ref(case), &mut log)?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires disposable Strict servers and the native LOCATION producer"]
async fn parity_powder_snow_location_advancement() -> Result<()> {
    let case = target()?;
    ferraloom_parity::run(options("Loc", 1), RunMode::from_env()?, async |session| {
        sweep(session, &case).await
    })
    .await
}
