use super::*;

async fn bone_meal(bot: &mut Bot, pos: BlockPos) -> Result<()> {
    bot.use_item_on(
        InteractionHand::MainHand,
        BlockHitResult {
            block_pos: pos,
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 1.0,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await?;
    bot.synchronize().await
}

pub(super) async fn bonemeal(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    command(
        bot,
        "give ParityBot minecraft:bone_meal 64",
        "commands.give.success.single",
    )
    .await?;
    fill(
        bot,
        at(2, 2).below(1),
        at(14, 14).below(1),
        Block::GRASS_BLOCK.default_state(),
    )
    .await?;
    let grass = at(10, 8).below(1);
    bone_meal(bot, grass).await?;
    step(bot, 1).await?;
    let mut vegetation = false;
    for x in 2..=14 {
        for z in 2..=14 {
            vegetation |= !block(bot, at(x, z))?.is_air();
        }
    }
    ensure!(vegetation, "bone meal placed no grass or flowers");
    log.record("bonemeal.grass_places_vegetation", vegetation)?;
    fill(
        bot,
        at(-8, -8),
        BlockPos::new(23, -32, 23),
        Block::AIR.default_state(),
    )
    .await?;
    other_targets(bot, log).await?;
    fill(
        bot,
        at(-8, -8),
        BlockPos::new(23, -32, 23),
        Block::AIR.default_state(),
    )
    .await?;
    let sapling = at(10, 10);
    put(bot, sapling.below(1), Block::DIRT.default_state()).await?;
    put(bot, sapling, Block::OAK_SAPLING.default_state()).await?;
    for _ in 0..40 {
        if block(bot, sapling)?.block() == Block::OAK_LOG {
            break;
        }
        bone_meal(bot, sapling).await?;
    }
    ensure!(
        block(bot, sapling)?.block() == Block::OAK_LOG,
        "bone meal did not generate an oak tree"
    );
    record(log, bot, "bonemeal.sapling_generates_live_tree", &[sapling])?;

    // A low solid ceiling forces configured-feature failure; TreeGrower must
    // put back the captured stage-one sapling rather than losing the plant.
    reset(bot).await?;
    fill(
        bot,
        at(7, 7).above(2),
        at(13, 13).above(2),
        Block::STONE.default_state(),
    )
    .await?;
    put(bot, sapling.below(1), Block::DIRT.default_state()).await?;
    let captured = state(Block::OAK_SAPLING, &[("stage", "1")]);
    put(bot, sapling, captured).await?;
    for _ in 0..12 {
        bone_meal(bot, sapling).await?;
    }
    expect_blocks(bot, &[(sapling, captured)]).await?;
    record(
        log,
        bot,
        "bonemeal.failed_tree_restores_sapling",
        &[sapling],
    )
}

async fn other_targets(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    let pitcher = at(10, 8);
    put(bot, pitcher.below(1), Block::FARMLAND.default_state()).await?;
    put(bot, pitcher, Block::PITCHER_CROP.default_state()).await?;
    for age in 1..=2 {
        bone_meal(bot, pitcher).await?;
        expect_blocks(
            bot,
            &[(
                pitcher,
                Block::PITCHER_CROP
                    .default_state()
                    .with(p::AGE, age)
                    .unwrap(),
            )],
        )
        .await?;
    }
    put(bot, pitcher.above(1), Block::STONE.default_state()).await?;
    bone_meal(bot, pitcher).await?;
    ensure!(
        block(bot, pitcher)?.get(p::AGE) == Some(2),
        "pitcher grew through its upper obstruction"
    );
    record(log, bot, "bonemeal.pitcher_blocked_at_two", &[pitcher])?;
    put(bot, pitcher.above(1), Block::AIR.default_state()).await?;
    bone_meal(bot, pitcher).await?;
    let lower = state(Block::PITCHER_CROP, &[("age", "3"), ("half", "lower")]);
    let upper = state(Block::PITCHER_CROP, &[("age", "3"), ("half", "upper")]);
    expect_blocks(bot, &[(pitcher, lower), (pitcher.above(1), upper)]).await?;
    record(
        log,
        bot,
        "bonemeal.pitcher_creates_upper_half",
        &[pitcher, pitcher.above(1)],
    )?;
    bone_meal(bot, pitcher.above(1)).await?;
    expect_blocks(
        bot,
        &[
            (pitcher, lower.with(p::AGE, 4).unwrap()),
            (pitcher.above(1), upper.with(p::AGE, 4).unwrap()),
        ],
    )
    .await?;
    record(
        log,
        bot,
        "bonemeal.pitcher_upper_redirects_to_lower",
        &[pitcher, pitcher.above(1)],
    )?;

    let berry = at(7, 10);
    put(bot, berry, state(Block::SWEET_BERRY_BUSH, &[("age", "2")])).await?;
    bone_meal(bot, berry).await?;
    expect_blocks(
        bot,
        &[(berry, state(Block::SWEET_BERRY_BUSH, &[("age", "3")]))],
    )
    .await?;
    record(log, bot, "bonemeal.berry_grows_before_harvest", &[berry])?;
    let leaves = at(11, 10).above(1);
    put(
        bot,
        leaves,
        state(Block::MANGROVE_LEAVES, &[("persistent", "true")]),
    )
    .await?;
    bone_meal(bot, leaves).await?;
    expect_blocks(
        bot,
        &[(
            leaves.below(1),
            state(Block::MANGROVE_PROPAGULE, &[("hanging", "true")]),
        )],
    )
    .await?;
    record(
        log,
        bot,
        "bonemeal.mangrove_hanging_propagule",
        &[leaves.below(1)],
    )
}

pub(super) async fn dripleaf(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let leaf = at(10, 8);
    put(bot, leaf.below(1), Block::CLAY.default_state()).await?;
    put(bot, leaf, Block::BIG_DRIPLEAF.default_state()).await?;
    put(bot, leaf.east(1), Block::REDSTONE_BLOCK.default_state()).await?;
    bot.teleport([10.5, -59.0625, 8.5]).await?;
    bot.move_to([10.5, -59.0625, 8.5], true).await?;
    step(bot, 2).await?;
    ensure!(
        block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::None),
        "powered dripleaf tilted"
    );
    record(log, bot, "dripleaf.power_suppresses_contact", &[leaf])?;
    put(bot, leaf.east(1), Block::AIR.default_state()).await?;
    bot.move_to([10.5, -59.0625, 8.5], true).await?;
    until(
        bot,
        "standing player did not make dripleaf unstable",
        4,
        1,
        |bot| Ok(block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::Unstable)),
    )
    .await?;
    record(log, bot, "dripleaf.contact_unstable", &[leaf])?;
    bot.teleport([7.5, -60.0, 8.5]).await?;
    step(bot, 10).await?;
    ensure!(
        block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::Partial),
        "dripleaf missed partial tilt"
    );
    record(log, bot, "dripleaf.partial_after_ten", &[leaf])?;
    step(bot, 10).await?;
    ensure!(
        block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::Full),
        "dripleaf missed full tilt"
    );
    record(log, bot, "dripleaf.full_after_twenty", &[leaf])?;
    step(bot, 100).await?;
    ensure!(
        block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::None),
        "dripleaf failed to reset"
    );
    record(log, bot, "dripleaf.reset_after_full", &[leaf])?;

    bot.teleport([10.5, -59.0625, 8.5]).await?;
    bot.move_to([10.5, -59.0625, 8.5], true).await?;
    until(
        bot,
        "second dripleaf contact was not detected",
        4,
        1,
        |bot| Ok(block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::Unstable)),
    )
    .await?;
    put(bot, leaf.east(1), Block::REDSTONE_BLOCK.default_state()).await?;
    ensure!(
        block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::None),
        "power failed to reset pending tilt"
    );
    bot.teleport([7.5, -60.0, 8.5]).await?;
    step(bot, 12).await?;
    ensure!(
        block(bot, leaf)?.get(p::TILT) == Some(p::Tilt::None),
        "stale scheduled tilt ignored live power"
    );
    record(log, bot, "dripleaf.power_cancels_pending_tilt", &[leaf])
}
