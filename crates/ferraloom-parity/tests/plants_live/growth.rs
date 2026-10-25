use super::*;

pub(super) async fn crops(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let crops = [
        Block::WHEAT,
        Block::CARROTS,
        Block::POTATOES,
        Block::BEETROOTS,
    ];
    let mut grown = Vec::new();
    for (index, crop) in crops.into_iter().enumerate() {
        let pos = at(3 + index as i32 * 3, 3);
        put(
            bot,
            pos.below(1),
            state(Block::FARMLAND, &[("moisture", "7")]),
        )
        .await?;
        put(bot, pos.south(1).below(1), Block::WATER.default_state()).await?;
        put(bot, pos.south(1), Block::GLOWSTONE.default_state()).await?;
        put(bot, pos, crop.default_state()).await?;
        grown.push((pos, if crop == Block::BEETROOTS { 3 } else { 7 }));
    }
    let dry = at(15, 15);
    put(bot, dry.below(1), Block::FARMLAND.default_state()).await?;
    put(bot, dry, Block::WHEAT.default_state()).await?;
    grown.push((dry, 7));
    // A dense square takes the same-row/column and diagonal growth-speed branch.
    for pos in [at(9, 10), at(10, 10), at(9, 11), at(10, 11)] {
        put(
            bot,
            pos.below(1),
            state(Block::FARMLAND, &[("moisture", "7")]),
        )
        .await?;
        put(bot, pos, Block::WHEAT.default_state()).await?;
        grown.push((pos, 7));
    }
    put(bot, at(9, 13).below(1), Block::WATER.default_state()).await?;
    put(bot, at(9, 13), Block::GLOWSTONE.default_state()).await?;
    // CropBlock needs raw light 8 to survive, but 9 to grow. The sealed room
    // excludes skylight; an adjacent level-9 light block gives the crop 8.
    fill(
        bot,
        at(2, 8),
        BlockPos::new(6, Y + 3, 12),
        Block::STONE.default_state(),
    )
    .await?;
    fill(
        bot,
        at(3, 9),
        BlockPos::new(5, Y + 2, 11),
        Block::AIR.default_state(),
    )
    .await?;
    let dim = at(4, 10);
    put(bot, dim.west(1), state(Block::LIGHT, &[("level", "9")])).await?;
    step(bot, 4).await?;
    put(
        bot,
        dim.below(1),
        state(Block::FARMLAND, &[("moisture", "7")]),
    )
    .await?;
    put(bot, dim, Block::WHEAT.default_state()).await?;
    step(bot, 4).await?;
    random_ticks(bot, 4096).await?;
    until(
        bot,
        "crops did not reach native maximum ages",
        40,
        20,
        |bot| {
            for (pos, age) in &grown {
                if block(bot, *pos)?.get(p::AGE) != Some(*age) {
                    return Ok(false);
                }
            }
            Ok(true)
        },
    )
    .await?;
    ensure!(
        block(bot, dim)? == Block::WHEAT.default_state(),
        "crop changed at the survival-only light threshold: {:?}",
        block(bot, dim)?
    );
    record(
        log,
        bot,
        "crops.moisture_and_crowding_max_ages",
        &grown.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
    )?;
    record(log, bot, "crops.light_eight_control", &[dim])
}

pub(super) async fn columns(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let cane = at(3, 3);
    put(bot, cane.below(1), Block::SAND.default_state()).await?;
    put(bot, cane.east(1).below(1), Block::WATER.default_state()).await?;
    put(bot, cane.east(1), Block::GLOWSTONE.default_state()).await?;
    for dy in 0..2 {
        put(
            bot,
            cane.above(dy),
            state(Block::SUGAR_CANE, &[("age", "15")]),
        )
        .await?;
    }
    let cactus = at(9, 3);
    put(bot, cactus.below(1), Block::SAND.default_state()).await?;
    for dy in 0..2 {
        put(
            bot,
            cactus.above(dy),
            state(Block::CACTUS, &[("age", "15")]),
        )
        .await?;
    }
    let bamboo = at(9, 10);
    put(bot, bamboo.below(1), Block::DIRT.default_state()).await?;
    fill(
        bot,
        bamboo,
        bamboo.above(14),
        state(Block::BAMBOO, &[("age", "1")]),
    )
    .await?;
    random_ticks(bot, 4096).await?;
    until(bot, "column heads did not grow", 20, 10, |bot| {
        Ok(block(bot, cane.above(2))?.block() == Block::SUGAR_CANE
            && block(bot, cactus.above(2))?.block() == Block::CACTUS
            && block(bot, bamboo.above(15))?.block() == Block::BAMBOO)
    })
    .await?;
    step(bot, 32).await?;
    ensure!(
        block(bot, cane.above(3))?.is_air(),
        "cane exceeded three blocks"
    );
    ensure!(
        block(bot, cactus.above(3))?.block() != Block::CACTUS,
        "cactus exceeded three blocks"
    );
    ensure!(
        block(bot, bamboo.above(16))?.is_air(),
        "bamboo exceeded sixteen blocks"
    );
    ensure!(
        block(bot, bamboo.above(15))?.get(p::STAGE) == Some(1),
        "bamboo did not stop at maximum height"
    );
    // Ages and optional cactus flowers vary with random ticks; height caps and
    // the bamboo terminal stage are deterministic endpoints on both servers.
    log.record("columns.height_limits", [3, 3, 16])?;
    record(
        log,
        bot,
        "columns.bamboo_terminal_head",
        &[bamboo.above(15)],
    )?;
    random_ticks(bot, 0).await?;
    put(bot, cane.east(1).below(1), Block::AIR.default_state()).await?;
    // Water is diagonal from the cane. SugarCaneBlock only schedules its
    // survival tick when its own updateShape runs; changing the sand's neighbor
    // does not propagate an unchanged sand state into another shape update.
    step(bot, 2).await?;
    ensure!(
        (0..3).all(|dy| block(bot, cane.above(dy)).is_ok_and(|s| s.block() == Block::SUGAR_CANE)),
        "diagonal water removal unexpectedly updated cane"
    );
    record(
        log,
        bot,
        "columns.cane_water_loss_waits_for_shape_update",
        &[cane, cane.above(1), cane.above(2)],
    )?;
    put(bot, cane.east(1), Block::AIR.default_state()).await?;
    step(bot, 4).await?;
    expect_blocks(
        bot,
        &(0..3)
            .map(|dy| (cane.above(dy), Block::AIR.default_state()))
            .collect::<Vec<_>>(),
    )
    .await?;
    record(
        log,
        bot,
        "columns.cane_requires_water_after_shape_update",
        &[cane, cane.above(1), cane.above(2)],
    )
}

pub(super) async fn vines_and_kelp(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    reset(bot).await?;
    let vine = at(4, 4).above(4);
    fill(bot, at(4, 3), vine.north(1), Block::STONE.default_state()).await?;
    put(bot, vine, state(Block::VINE, &[("north", "true")])).await?;
    command(
        bot,
        "gamerule minecraft:spread_vines false",
        "commands.gamerule.set",
    )
    .await?;
    random_ticks(bot, 4096).await?;
    step(bot, 32).await?;
    ensure!(block(bot, vine.below(1))?.is_air(), "disabled vines spread");
    command(
        bot,
        "gamerule minecraft:spread_vines true",
        "commands.gamerule.set",
    )
    .await?;
    until(bot, "wall vine did not extend downward", 24, 20, |bot| {
        Ok(block(bot, vine.below(1))?.block() == Block::VINE)
    })
    .await?;
    log.record("vines.gamerule_and_downward_growth", true)?;

    random_ticks(bot, 0).await?;
    let kelp = at(10, 9);
    // Glass contains the source column; the luminous roof prevents cold-biome
    // precipitation from freezing the water while the kelp is growing.
    fill(
        bot,
        kelp.offset(-1, 0, -1),
        kelp.offset(1, 4, 1),
        Block::GLASS.default_state(),
    )
    .await?;
    fill(bot, kelp, kelp.above(3), Block::WATER.default_state()).await?;
    put(bot, kelp.above(4), Block::GLOWSTONE.default_state()).await?;
    put(bot, kelp, state(Block::KELP, &[("age", "0")])).await?;
    random_ticks(bot, 4096).await?;
    until(
        bot,
        "kelp did not grow through its source water",
        20,
        20,
        |bot| Ok(block(bot, kelp.above(3))?.block() == Block::KELP),
    )
    .await?;
    random_ticks(bot, 0).await?;
    for dy in 0..3 {
        ensure!(
            block(bot, kelp.above(dy))?.block() == Block::KELP_PLANT,
            "kelp failed to turn its old head into a body"
        );
    }
    ensure!(
        block(bot, kelp.above(4))?.block() == Block::GLOWSTONE,
        "kelp grew through its obstruction"
    );
    record(
        log,
        bot,
        "kelp.old_heads_become_bodies",
        &[kelp, kelp.above(1), kelp.above(2)],
    )?;
    log.record("kelp.source_water_growth_bounded_by_roof", true)?;
    put(bot, kelp.below(1), Block::AIR.default_state()).await?;
    step(bot, 8).await?;
    ensure!(
        (0..4).all(|dy| block(bot, kelp.above(dy)).is_ok_and(|s| s.block() == Block::WATER)),
        "kelp support loss did not restore water"
    );
    log.record("kelp.support_loss_restores_water", true)
}
