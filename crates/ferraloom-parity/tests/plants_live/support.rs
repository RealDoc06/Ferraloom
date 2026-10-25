use super::*;

pub(super) async fn survival(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let mut removed = Vec::new();
    for (index, plant) in [Block::DANDELION, Block::SHORT_GRASS, Block::FERN]
        .into_iter()
        .enumerate()
    {
        let pos = at(3 + index as i32 * 3, 3);
        put(bot, pos.below(1), Block::DIRT.default_state()).await?;
        put(bot, pos, plant.default_state()).await?;
        expect_blocks(bot, &[(pos, plant.default_state())]).await?;
        put(bot, pos.below(1), Block::AIR.default_state()).await?;
        removed.push(pos);
    }
    for (index, plant) in [Block::LARGE_FERN, Block::SUNFLOWER]
        .into_iter()
        .enumerate()
    {
        let pos = at(3 + index as i32 * 4, 7);
        put(bot, pos.below(1), Block::DIRT.default_state()).await?;
        put(bot, pos, state(plant, &[("half", "lower")])).await?;
        put(bot, pos.above(1), state(plant, &[("half", "upper")])).await?;
        put(bot, pos.below(1), Block::AIR.default_state()).await?;
        removed.extend([pos, pos.above(1)]);
    }
    let cane = at(11, 7);
    put(bot, cane.below(1), Block::SAND.default_state()).await?;
    put(bot, cane.east(1).below(1), Block::WATER.default_state()).await?;
    put(bot, cane.east(1), Block::GLOWSTONE.default_state()).await?;
    put(bot, cane, Block::SUGAR_CANE.default_state()).await?;
    put(bot, cane.below(1), Block::AIR.default_state()).await?;
    step(bot, 2).await?;
    removed.push(cane);
    expect_blocks(
        bot,
        &removed
            .iter()
            .map(|p| (*p, Block::AIR.default_state()))
            .collect::<Vec<_>>(),
    )
    .await?;
    record(log, bot, "support.removed", &removed)?;

    let lichen = at(4, 11);
    put(bot, lichen.north(1), Block::STONE.default_state()).await?;
    put(bot, lichen.east(1), Block::STONE.default_state()).await?;
    put(
        bot,
        lichen,
        state(
            Block::GLOW_LICHEN,
            &[("north", "true"), ("east", "true"), ("waterlogged", "true")],
        ),
    )
    .await?;
    put(bot, lichen.north(1), Block::AIR.default_state()).await?;
    let remaining = state(
        Block::GLOW_LICHEN,
        &[("east", "true"), ("waterlogged", "true")],
    );
    expect_blocks(bot, &[(lichen, remaining)]).await?;
    record(log, bot, "support.lichen_keeps_other_face", &[lichen])?;
    put(bot, lichen.east(1), Block::AIR.default_state()).await?;
    expect_blocks(bot, &[(lichen, Block::WATER.default_state())]).await?;
    record(log, bot, "support.lichen_restores_water", &[lichen])
}

pub(super) async fn leaves(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let wood = at(3, 4);
    let first = wood.east(1);
    let second = wood.east(2);
    let persistent = at(8, 4);
    let wet = at(11, 11);
    put(bot, wood, Block::OAK_LOG.default_state()).await?;
    for pos in [first, second] {
        put(bot, pos, Block::OAK_LEAVES.default_state()).await?;
    }
    put(
        bot,
        persistent,
        state(Block::OAK_LEAVES, &[("persistent", "true")]),
    )
    .await?;
    put(
        bot,
        wet,
        state(Block::OAK_LEAVES, &[("waterlogged", "true")]),
    )
    .await?;
    put(bot, wet.above(1), Block::GLOWSTONE.default_state()).await?;
    step(bot, 8).await?;
    ensure!(
        block(bot, first)?.get(p::DISTANCE) == Some(1),
        "adjacent leaf distance"
    );
    ensure!(
        block(bot, second)?.get(p::DISTANCE) == Some(2),
        "propagated leaf distance"
    );
    record(log, bot, "leaves.connected", &[first, second, persistent])?;
    random_ticks(bot, 4096).await?;
    until(
        bot,
        "waterlogged disconnected leaf did not decay",
        12,
        8,
        |bot| Ok(block(bot, wet)?.block() == Block::WATER),
    )
    .await?;
    ensure!(
        block(bot, first)?.block() == Block::OAK_LEAVES,
        "connected leaf decayed"
    );
    ensure!(
        block(bot, persistent)?.get(p::PERSISTENT) == Some(true),
        "persistent leaf decayed"
    );
    record(log, bot, "leaves.wet_decay", &[wet])?;
    random_ticks(bot, 0).await?;
    put(bot, wood, Block::AIR.default_state()).await?;
    step(bot, 8).await?;
    ensure!(
        [first, second]
            .into_iter()
            .all(|p| block(bot, p).is_ok_and(|s| s.get(p::DISTANCE) == Some(7))),
        "log removal did not propagate distance seven"
    );
    random_ticks(bot, 4096).await?;
    until(bot, "unsupported leaves did not decay", 12, 8, |bot| {
        Ok(block(bot, first)?.is_air() && block(bot, second)?.is_air())
    })
    .await?;
    record(
        log,
        bot,
        "leaves.after_log_removed",
        &[first, second, persistent],
    )
}
