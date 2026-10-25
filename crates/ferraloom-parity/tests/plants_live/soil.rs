use super::*;

pub(super) async fn spreading(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let mut patches = Vec::new();
    for (source, center) in [
        (Block::GRASS_BLOCK, at(4, 4).below(1)),
        (Block::MYCELIUM, at(11, 11).below(1)),
    ] {
        fill(
            bot,
            center.offset(-1, 0, -1),
            center.offset(1, 0, 1),
            Block::DIRT.default_state(),
        )
        .await?;
        put(bot, center, source.default_state()).await?;
        patches.push((source, center));
    }
    let covered = at(4, 11).below(1);
    let drowned = at(7, 11).below(1);
    let snowy = at(11, 4).below(1);
    for pos in [covered, drowned, snowy] {
        put(bot, pos, Block::GRASS_BLOCK.default_state()).await?;
    }
    put(bot, covered.above(1), Block::STONE.default_state()).await?;
    put(bot, drowned.above(1), Block::WATER.default_state()).await?;
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        put(
            bot,
            drowned.above(1).relative(direction, 1),
            Block::GLASS.default_state(),
        )
        .await?;
    }
    put(bot, drowned.above(2), Block::GLOWSTONE.default_state()).await?;
    put(bot, snowy.above(1), Block::SNOW.default_state()).await?;
    step(bot, 4).await?;
    random_ticks(bot, 4096).await?;
    until(
        bot,
        "spreading soil did not fill exposed dirt or decay under covers",
        24,
        20,
        |bot| {
            for (source, center) in &patches {
                for x in -1..=1 {
                    for z in -1..=1 {
                        if block(bot, center.offset(x, 0, z))?.block() != *source {
                            return Ok(false);
                        }
                    }
                }
            }
            Ok(block(bot, covered)?.block() == Block::DIRT
                && block(bot, drowned)?.block() == Block::DIRT)
        },
    )
    .await?;
    ensure!(
        block(bot, snowy)?.block() == Block::GRASS_BLOCK
            && block(bot, snowy)?.get(p::SNOWY) == Some(true),
        "one snow layer killed grass or failed snowy update"
    );
    for (source, center) in patches {
        let positions: Vec<_> = (-1..=1)
            .flat_map(|x| (-1..=1).map(move |z| center.offset(x, 0, z)))
            .collect();
        record(
            log,
            bot,
            &format!("spreading.{}", source.name()),
            &positions,
        )?;
    }
    record(
        log,
        bot,
        "spreading.cover_controls",
        &[covered, drowned, snowy],
    )
}

pub(super) async fn farmland(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let wet = at(3, 3).below(1);
    let dry = at(12, 3).below(1);
    let protected = at(12, 11).below(1);
    put(bot, wet, Block::FARMLAND.default_state()).await?;
    put(bot, wet.east(4), Block::WATER.default_state()).await?;
    put(bot, wet.east(4).above(1), Block::GLOWSTONE.default_state()).await?;
    for pos in [dry, protected] {
        put(bot, pos, state(Block::FARMLAND, &[("moisture", "7")])).await?;
    }
    put(
        bot,
        protected.above(1),
        state(Block::WHEAT, &[("age", "7")]),
    )
    .await?;
    random_ticks(bot, 4096).await?;
    until(
        bot,
        "farmland did not hydrate at four blocks or dry away from water",
        20,
        20,
        |bot| {
            Ok(block(bot, wet)?.get(p::MOISTURE) == Some(7)
                && block(bot, dry)?.block() == Block::DIRT
                && block(bot, protected)?.get(p::MOISTURE) == Some(0))
        },
    )
    .await?;
    record(
        log,
        bot,
        "farmland.hydration_and_crop_protection",
        &[wet, dry, protected],
    )?;
    random_ticks(bot, 0).await?;
    let path = at(3, 10).below(1);
    put(bot, path, Block::DIRT_PATH.default_state()).await?;
    put(bot, path.above(1), Block::STONE.default_state()).await?;
    step(bot, 2).await?;
    expect_blocks(bot, &[(path, Block::DIRT.default_state())]).await?;
    record(log, bot, "farmland.path_cover_conversion", &[path])?;

    // More than 1.5 blocks of real client descent makes the native trample
    // probability one. The final on-ground packet invokes the actual fall path.
    let trampled = at(8, 8).below(1);
    put(bot, trampled, state(Block::FARMLAND, &[("moisture", "7")])).await?;
    bot.teleport([8.5, -56.0, 8.5]).await?;
    bot.move_to([8.5, -60.0625, 8.5], false).await?;
    bot.move_to([8.5, -60.0625, 8.5], true).await?;
    expect_blocks(bot, &[(trampled, Block::DIRT.default_state())]).await?;
    record(log, bot, "farmland.real_player_fall", &[trampled])?;
    log.record(
        "farmland.player_after_collision_expands",
        bot.snapshot().position,
    )
}
