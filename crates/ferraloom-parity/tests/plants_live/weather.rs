use super::*;

async fn contain(bot: &mut Bot, pos: BlockPos) -> Result<()> {
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        put(
            bot,
            pos.relative(direction, 1),
            Block::GLASS.default_state(),
        )
        .await?;
    }
    put(bot, pos.above(1), Block::GLOWSTONE.default_state()).await
}

pub(super) async fn melting(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let frosted = at(11, 3);
    contain(bot, frosted).await?;
    put(bot, frosted, state(Block::FROSTED_ICE, &[("age", "2")])).await?;
    // With random ticks disabled this can only be the scheduled age/melt path.
    until(
        bot,
        "frosted ice did not age and melt on scheduled ticks",
        12,
        20,
        |bot| Ok(block(bot, frosted)?.block() == Block::WATER),
    )
    .await?;
    record(log, bot, "ice.scheduled_frosted_melt", &[frosted])?;
    let ice = at(3, 10);
    contain(bot, ice).await?;
    put(bot, ice, Block::ICE.default_state()).await?;
    let bright = at(5, 5);
    let dim = at(11, 11);
    put(bot, bright.south(1), Block::GLOWSTONE.default_state()).await?;
    put(bot, dim.west(4), Block::GLOWSTONE.default_state()).await?;
    for pos in [bright, dim] {
        put(bot, pos, Block::SNOW.default_state()).await?;
    }
    let packed = at(13, 7);
    let blue = at(13, 9);
    put(bot, packed, Block::PACKED_ICE.default_state()).await?;
    put(bot, blue, Block::BLUE_ICE.default_state()).await?;
    put(bot, at(13, 8), Block::GLOWSTONE.default_state()).await?;
    step(bot, 4).await?;
    random_ticks(bot, 4096).await?;
    until(bot, "bright snow or ice did not melt", 16, 8, |bot| {
        Ok(block(bot, bright)?.is_air() && block(bot, ice)?.block() == Block::WATER)
    })
    .await?;
    step(bot, 16).await?;
    ensure!(
        block(bot, dim)?.block() == Block::SNOW,
        "snow melted at block light eleven"
    );
    ensure!(
        block(bot, packed)?.block() == Block::PACKED_ICE,
        "packed ice melted"
    );
    ensure!(
        block(bot, blue)?.block() == Block::BLUE_ICE,
        "blue ice melted"
    );
    record(
        log,
        bot,
        "ice_and_snow.light_thresholds",
        &[bright, dim, ice, packed, blue],
    )
}

pub(super) async fn precipitation(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    // Half a chunk exposes source-water edges; the other half exposes solid
    // snow supports. Compare existence, not nondeterministic precipitation cells.
    fill(
        bot,
        at(1, 1).below(1),
        at(7, 14).below(1),
        Block::WATER.default_state(),
    )
    .await?;
    command(bot, "weather rain 100000", "commands.weather.set.rain").await?;
    random_ticks(bot, 4096).await?;
    until(
        bot,
        "cold precipitation failed to freeze water and accumulate snow",
        20,
        20,
        |bot| {
            let snapshot = bot.snapshot();
            let mut frozen = false;
            let mut snow = false;
            for x in 1..=14 {
                for z in 1..=14 {
                    let below = snapshot
                        .block(at(x, z).below(1))
                        .context("cold floor not loaded")?;
                    let above = snapshot
                        .block(at(x, z))
                        .context("cold surface not loaded")?;
                    frozen |= below.block() == Block::ICE;
                    snow |= above.block() == Block::SNOW;
                }
            }
            Ok(frozen && snow)
        },
    )
    .await?;
    log.record("cold.precipitation_freezes_source_water", true)?;
    log.record("cold.precipitation_accumulates_snow", true)
}
