#[allow(dead_code)] // Each parity target uses a subset of the shared level fixtures.
mod level_support;
#[allow(dead_code)]
mod redstone_support;

use ferraloom_data::{Block, properties as p};
use ferraloom_parity::{Bot, ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::BlockPos;
use level_support::*;
use redstone_support::set;

async fn build_piston(bot: &mut Bot, pos: BlockPos, sticky: bool, length: i32) -> Result<()> {
    let block = if sticky { "sticky_piston" } else { "piston" };
    command(
        bot,
        &format!(
            "setblock {} {} {} minecraft:{block}[facing=east]",
            pos.x, pos.y, pos.z
        ),
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        &format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            pos.x + 1,
            pos.y,
            pos.z,
            pos.x + length,
            pos.y,
            pos.z
        ),
        "commands.fill.success",
    )
    .await?;
    Ok(())
}

fn record_rows(bot: &Bot, log: &mut ObservationLog, name: String, rows: &[BlockPos]) -> Result<()> {
    let states = rows
        .iter()
        .map(|pos| {
            (0..=15)
                .map(|x| Ok(block(bot, BlockPos::new(pos.x + x, pos.y, pos.z))?.id()))
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    log.record(name, states)
}

async fn record(session: &mut Session) -> Result<ObservationLog> {
    let bot_id = session.join("ParityBot").await?;
    let bot = session.bot(bot_id)?;
    redstone_support::prepare(bot).await?;
    command(
        bot,
        "fill 4 -60 4 32 -60 24 minecraft:stone strict",
        "commands.fill.success",
    )
    .await?;
    let mut log = ObservationLog::default();
    let limit = [BlockPos::new(8, -59, 8), BlockPos::new(8, -59, 12)];
    for (pos, length) in limit.into_iter().zip([12, 13]) {
        build_piston(bot, pos, false, length).await?;
        set(bot, pos.below(1), Block::REDSTONE_BLOCK).await?;
    }
    for tick in 1..=7 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("push-limit.{tick}"), &limit)?;
    }
    ensure!(
        block(bot, limit[0])?.get(p::EXTENDED) == Some(true),
        "twelve blocks must move"
    );
    ensure!(
        block(bot, limit[0].relative(ferraloom_util::Direction::East, 1))?.block()
            == Block::PISTON_HEAD
    );
    ensure!(
        block(bot, limit[0].relative(ferraloom_util::Direction::East, 13))?.block() == Block::STONE
    );
    ensure!(
        block(bot, limit[1])?.get(p::EXTENDED) == Some(false),
        "thirteen blocks must stay"
    );
    ensure!(
        block(bot, limit[1].relative(ferraloom_util::Direction::East, 14))?.block() == Block::AIR
    );

    let quasi = BlockPos::new(8, -59, 16);
    let wake = BlockPos::new(7, -59, 16);
    build_piston(bot, quasi, true, 1).await?;
    set(bot, quasi.above(2), Block::REDSTONE_BLOCK).await?;
    for tick in 1..=3 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.no-update.{tick}"), &[quasi])?;
    }
    ensure!(
        block(bot, quasi)?.get(p::EXTENDED) == Some(false),
        "quasi power needs a neighbor update"
    );
    set(bot, wake, Block::STONE).await?;
    for tick in 1..=6 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.extend.{tick}"), &[quasi])?;
    }
    ensure!(block(bot, quasi)?.get(p::EXTENDED) == Some(true));
    set(bot, quasi.above(2), Block::AIR).await?;
    for tick in 1..=3 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.unpowered.{tick}"), &[quasi])?;
    }
    ensure!(
        block(bot, quasi)?.get(p::EXTENDED) == Some(true),
        "quasi removal also needs a neighbor update"
    );
    set(bot, wake, Block::AIR).await?;
    for tick in 1..=6 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.retract.{tick}"), &[quasi])?;
    }
    ensure!(block(bot, quasi)?.get(p::EXTENDED) == Some(false));
    ensure!(
        block(bot, quasi.relative(ferraloom_util::Direction::East, 1))?.block() == Block::STONE
    );

    let pulses = [BlockPos::new(8, -59, 20), BlockPos::new(8, -59, 24)];
    for pos in pulses {
        build_piston(bot, pos, true, 1).await?;
        set(bot, pos.below(1), Block::REDSTONE_BLOCK).await?;
    }
    // The second extension event is queued and loses power before its phase.
    set(bot, pulses[1].below(1), Block::AIR).await?;
    step(bot, 1).await?;
    record_rows(bot, &mut log, "short-pulse.extend.1".into(), &pulses)?;
    set(bot, pulses[0].below(1), Block::AIR).await?;
    for tick in 1..=8 {
        step(bot, 1).await?;
        record_rows(
            bot,
            &mut log,
            format!("short-pulse.retract.{tick}"),
            &pulses,
        )?;
    }
    for pos in pulses {
        ensure!(block(bot, pos)?.get(p::EXTENDED) == Some(false));
    }
    ensure!(
        block(bot, pulses[1].relative(ferraloom_util::Direction::East, 1))?.block() == Block::STONE
    );
    ensure!(
        block(bot, pulses[1].relative(ferraloom_util::Direction::East, 2))?.block() == Block::AIR
    );
    sticky_branches(bot, &mut log).await?;
    collision_and_drop_order(bot, &mut log).await?;
    immovable(bot, &mut log).await?;
    contraction_recheck(bot, &mut log).await?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK and integrated live piston phases"]
async fn parity_piston_push_limits_quasi_updates_and_short_pulses() -> Result<()> {
    run(redstone_support::options(), RunMode::Both, record).await
}

async fn sticky_branches(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    use redstone_support::{sample, source, state, timeline};
    for (case, (sticky, other)) in [
        (Block::SLIME_BLOCK, Block::HONEY_BLOCK),
        (Block::HONEY_BLOCK, Block::SLIME_BLOCK),
    ]
    .into_iter()
    .enumerate()
    {
        let pos = BlockPos::new(32, -55, 8 + case as i32 * 10);
        state(bot, pos, "minecraft:sticky_piston[facing=east]").await?;
        let root = pos.east(1);
        set(bot, root, sticky).await?;
        set(bot, root.above(1), other).await?;
        for branch in [root.below(1), root.north(1), root.south(1)] {
            set(bot, branch, Block::STONE).await?;
        }
        let positions = [
            pos,
            root,
            root.east(1),
            root.above(1),
            root.above(1).east(1),
            root.below(1),
            root.below(1).east(1),
            root.north(1),
            root.north(1).east(1),
            root.south(1),
            root.south(1).east(1),
        ];
        sample(bot, log, format!("sticky.{case}.before"), &positions)?;
        source(bot, pos.west(1), true).await?;
        timeline(bot, log, &format!("sticky.{case}.extend"), &positions, 6).await?;
        ensure!(block(bot, root.east(1))?.block() == sticky);
        ensure!(
            block(bot, root.above(1))?.block() == other,
            "slime and honey stuck together"
        );
        ensure!(block(bot, root.north(1).east(1))?.block() == Block::STONE);
        source(bot, pos.west(1), false).await?;
        timeline(bot, log, &format!("sticky.{case}.retract"), &positions, 6).await?;
        ensure!(block(bot, root)?.block() == sticky);
    }
    for count in [12, 13] {
        let pos = BlockPos::new(8, -55, 32 + (count - 12) * 16);
        state(bot, pos, "minecraft:piston[facing=east]").await?;
        let mut positions = vec![pos];
        for z in 0..count {
            let branch = BlockPos::new(pos.x + 1, pos.y, pos.z + z);
            set(bot, branch, Block::SLIME_BLOCK).await?;
            positions.extend([branch, branch.east(1)]);
        }
        source(bot, pos.west(1), true).await?;
        timeline(bot, log, &format!("sticky.limit.{count}"), &positions, 6).await?;
        ensure!(
            block(bot, pos)?.get(p::EXTENDED) == Some(count == 12),
            "sticky branch total did not obey the 12-block limit"
        );
    }
    Ok(())
}
async fn collision_and_drop_order(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    use redstone_support::{source, state, timeline};
    let pos = BlockPos::new(36, -55, 36);
    state(bot, pos, "minecraft:piston[facing=east]").await?;
    let root = pos.east(1);
    // This branch folds above the piston and back behind an already queued
    // north line. Adding root.west(1).north(1) hits root.north(1), so native
    // reorderListAtCollision must put that new receiver before the old line.
    for cell in [
        root,
        root.above(1),
        root.above(1).west(1),
        root.above(1).west(1).north(1),
    ] {
        set(bot, cell, Block::SLIME_BLOCK).await?;
    }
    for cell in [root.north(1), root.west(1).north(1)] {
        set(bot, cell, Block::STONE).await?;
    }
    // Reverse destruction is visible as distinct, ordered native item spawns.
    let popped = [root.east(1), root.north(1).east(1), root.above(1).east(1)];
    for cell in &popped[..2] {
        set(bot, cell.below(1), Block::OBSIDIAN).await?;
        set(bot, *cell, Block::REDSTONE_WIRE).await?;
    }
    set(bot, popped[2], Block::COBWEB).await?;
    let mut positions = vec![pos];
    for y in 0..=1 {
        for x in 1..=5 {
            positions.push(BlockPos::new(pos.x + x, pos.y + y, pos.z));
        }
    }
    positions.extend(popped);
    let initial = game_time(bot).await?;
    source(bot, pos.west(1), true).await?;
    command(bot, "tick step 1", "commands.tick.step.success").await?;
    let mut spawned = std::collections::BTreeMap::new();
    let mut stacks = std::collections::BTreeMap::new();
    // Capture AddEntity's original coordinates, before random drop velocity
    // changes them. Its floor identifies the destroyed block without comparing RNG.
    bot.wait_for(
        "ordered piston pop receivers",
        ferraloom_parity::Ticks::DEFAULT,
        |_, packet| {
            use ferraloom_parity::protocol::{
                entity_data::EntityDataValue, play::clientbound::Packet,
            };
            match packet {
                Packet::AddEntity(entity)
                    if entity.entity_type.0
                        == i32::from(ferraloom_data::EntityType::ITEM.protocol_id()) =>
                {
                    spawned.insert(
                        entity.id.0,
                        (entity.x.floor() as i32, entity.z.floor() as i32),
                    );
                }
                Packet::SetEntityData(data) if spawned.contains_key(&data.id.0) => {
                    for value in &data.packed_items.0 {
                        if value.id == 8
                            && let EntityDataValue::ItemStack(stack) = &value.value
                        {
                            stacks.insert(
                                data.id.0,
                                (
                                    stack.item().protocol_id(),
                                    stack.0.as_ref().map_or(0, |stack| stack.count),
                                ),
                            );
                        }
                    }
                }
                _ => {}
            }
            stacks.len() == 3
        },
    )
    .await?;
    let drops = spawned
        .into_iter()
        .map(|(id, (x, z))| {
            let (item, count) = stacks[&id];
            (x, z, item, count)
        })
        .collect::<Vec<_>>();
    log.record("collision.popped-creation-order", drops)?;
    ensure!(game_time(bot).await? == initial + 1);
    timeline(bot, log, "collision.extend", &positions, 6).await?;
    ensure!(
        block(bot, pos)?.get(p::EXTENDED) == Some(true),
        "collision contraption did not extend"
    );
    Ok(())
}
async fn immovable(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    use redstone_support::{source, state, timeline};
    for (case, block_type) in [
        Block::OBSIDIAN,
        Block::BEDROCK,
        Block::REINFORCED_DEEPSLATE,
        Block::CHEST,
        Block::FURNACE,
    ]
    .into_iter()
    .enumerate()
    {
        let pos = BlockPos::new(48, -55, 4 + case as i32 * 5);
        state(bot, pos, "minecraft:piston[facing=east]").await?;
        set(bot, pos.east(1), block_type).await?;
        source(bot, pos.west(1), true).await?;
        timeline(
            bot,
            log,
            &format!("immovable.{case}"),
            &[pos, pos.east(1), pos.east(2)],
            4,
        )
        .await?;
        ensure!(block(bot, pos)?.get(p::EXTENDED) == Some(false));
    }
    let high = BlockPos::new(8, 318, 8);
    bot.teleport([8.5, 317.0, 11.5]).await?;
    state(bot, high, "minecraft:piston[facing=up]").await?;
    set(bot, high.above(1), Block::STONE).await?;
    source(bot, high.west(1), true).await?;
    timeline(
        bot,
        log,
        "immovable.world-height",
        &[high, high.above(1)],
        4,
    )
    .await?;
    ensure!(block(bot, high)?.get(p::EXTENDED) == Some(false));
    // Native isPushable checks the current block against the border. The
    // blocker itself must be outside: pushing an inside block into outside air
    // is allowed by addBlockLine's early air return. No synthetic border state.
    let edge = BlockPos::new(29_999_983, -59, 8);
    bot.teleport([29_999_980.5, -59.0, 11.5]).await?;
    state(bot, edge, "minecraft:piston[facing=east]").await?;
    set(bot, edge.east(1), Block::STONE).await?;
    source(bot, edge.west(1), true).await?;
    timeline(bot, log, "immovable.world-border", &[edge, edge.east(1)], 4).await?;
    ensure!(block(bot, edge)?.get(p::EXTENDED) == Some(false));
    bot.teleport([8.5, -59.0, 12.5]).await?;
    Ok(())
}
async fn contraction_recheck(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    use redstone_support::{source, state, timeline};
    let pos = BlockPos::new(32, -55, 52);
    state(bot, pos, "minecraft:sticky_piston[facing=east]").await?;
    set(bot, pos.east(1), Block::STONE).await?;
    source(bot, pos.west(1), true).await?;
    step(bot, 6).await?;
    source(bot, pos.west(1), false).await?;
    source(bot, pos.west(1), true).await?;
    timeline(
        bot,
        log,
        "block-event.cancel-retract",
        &[pos, pos.east(1), pos.east(2)],
        6,
    )
    .await?;
    ensure!(block(bot, pos)?.get(p::EXTENDED) == Some(true));
    source(bot, pos.west(1), false).await?;
    timeline(
        bot,
        log,
        "block-event.final-retract",
        &[pos, pos.east(1), pos.east(2)],
        6,
    )
    .await?;
    ensure!(block(bot, pos.east(1))?.block() == Block::STONE);
    Ok(())
}
