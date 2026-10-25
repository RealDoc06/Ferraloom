#[allow(dead_code)]
mod level_support;
#[allow(dead_code)]
mod redstone_support;
use ferraloom_data::{Block, Item, properties as p};
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, ensure,
    protocol::{
        codec::VarInt,
        play::{self, serverbound as sb},
    },
    run,
};
use ferraloom_util::{BlockPos, Direction};
use level_support::block;
use redstone_support::step;
use redstone_support::{click, data, hand, inventory, sample, set, source, state, timeline};

async fn hoppers(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    let origin = BlockPos::new(8, -59, 8);
    let positions = (0..4).map(|x| origin.east(x)).collect::<Vec<_>>();
    for pos in &positions {
        state(bot, *pos, "minecraft:hopper[facing=east]").await?;
        redstone_support::merge(bot, *pos, "{TransferCooldown:0}").await?;
    }
    state(bot, origin.east(4), "minecraft:barrel").await?;
    state(bot, origin.above(1), "minecraft:barrel").await?;
    redstone_support::merge(bot,origin.above(1),"{Items:[{Slot:0b,id:\"minecraft:dirt\",count:3},{Slot:1b,id:\"minecraft:stone\",count:2}]}").await?;
    // Allow the final source item to cross all four staggered eight-tick
    // cooldowns, including the two lock intervals above.
    for tick in 0..=96 {
        if [0, 24].contains(&tick) {
            source(bot, origin.north(1), true).await?;
        }
        if [10, 32].contains(&tick) {
            source(bot, origin.north(1), false).await?;
        }
        if tick > 0 {
            step(bot, 1).await?;
        }
        if tick > 0 {
            sample(bot, log, format!("hopper.{tick}.state"), &positions)?;
        }
        for (index, pos) in positions.iter().enumerate() {
            log.record(
                format!("hopper.{tick}.{index}.items"),
                inventory(bot, *pos).await?,
            )?;
            log.record(
                format!("hopper.{tick}.{index}.cooldown"),
                ferraloom_nbt::to_text(&data(bot, *pos, "TransferCooldown").await?, "", false),
            )?;
        }
        log.record(
            format!("hopper.{tick}.source"),
            inventory(bot, origin.above(1)).await?,
        )?;
        log.record(
            format!("hopper.{tick}.sink"),
            inventory(bot, origin.east(4)).await?,
        )?;
    }
    ensure!(
        inventory(bot, origin.above(1)).await?.is_empty(),
        "hopper source did not empty"
    );
    ensure!(
        inventory(bot, origin.east(4)).await?
            == [
                (0, "minecraft:dirt".into(), 3),
                (1, "minecraft:stone".into(), 2)
            ],
        "hopper chain lost items or source slot order"
    );
    Ok(())
}
async fn dispensers(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    for (case, name) in ["dispenser", "dropper"].into_iter().enumerate() {
        let pos = BlockPos::new(8 + case as i32 * 12, -59, 24);
        state(bot, pos, &format!("minecraft:{name}[facing=east]")).await?;
        redstone_support::merge(
            bot,
            pos,
            "{Items:[{Slot:0b,id:\"minecraft:stick\",count:6}]}",
        )
        .await?;
        // Stick selects native DefaultDispenseItemBehavior; stone selects the
        // separate sulfur-cube equipment behavior. Dropper uses a real sink.
        if name == "dropper" {
            set(bot, pos.east(1), Block::BARREL).await?;
        }
        source(bot, pos.above(2), true).await?;
        timeline(bot, log, &format!("{name}.quasi.sleep"), &[pos], 3).await?;
        ensure!(
            block(bot, pos)?.get(p::TRIGGERED) == Some(false),
            "quasi power invented a neighbor update"
        );
        set(bot, pos.west(1), Block::STONE).await?;
        for tick in 0..=8 {
            if tick > 0 {
                step(bot, 1).await?;
            }
            if tick > 0 {
                sample(bot, log, format!("{name}.first.{tick}"), &[pos])?;
            }
            let items = inventory(bot, pos).await?;
            ensure!(
                items.first().map(|item| item.2) == Some(if tick < 4 { 6 } else { 5 }),
                "{name} fired outside tick 4"
            );
            log.record(format!("{name}.first.{tick}.items"), items)?;
        }
        source(bot, pos.above(2), false).await?;
        set(bot, pos.west(1), Block::AIR).await?;
        ensure!(block(bot, pos)?.get(p::TRIGGERED) == Some(false));
        // Repeated pre-tick edges share one scheduled action.
        for _ in 0..3 {
            source(bot, pos.north(1), true).await?;
            source(bot, pos.north(1), false).await?;
        }
        for tick in 1..=8 {
            step(bot, 1).await?;
            log.record(format!("{name}.dedup.{tick}"), inventory(bot, pos).await?)?;
        }
        ensure!(inventory(bot, pos).await?.first().map(|item| item.2) == Some(4));
        if name == "dropper" {
            let destination = inventory(bot, pos.east(1)).await?;
            ensure!(destination == [(0, "minecraft:stick".into(), 2)]);
            log.record("dropper.destination", destination)?;
        } else {
            let snapshot = bot.snapshot();
            let counts = snapshot
                .entities
                .values()
                .filter_map(|entity| {
                    if entity.entity_type
                        != i32::from(ferraloom_data::EntityType::ITEM.protocol_id())
                    {
                        return None;
                    }
                    let ferraloom_parity::protocol::entity_data::EntityDataValue::ItemStack(stack) =
                        entity.data.get(&8)?
                    else {
                        return None;
                    };
                    (stack.item() == Item::STICK)
                        .then(|| stack.0.as_ref().map_or(0, |stack| stack.count))
                })
                .collect::<Vec<_>>();
            ensure!(
                counts.iter().sum::<i32>() == 2,
                "dispenser did not spawn both native default drops: {counts:?}"
            );
            log.record("dispenser.spawned_count", counts.iter().sum::<i32>())?;
        }
    }
    Ok(())
}
async fn openings(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    for (case, block_type) in [
        Block::OAK_DOOR,
        Block::IRON_DOOR,
        Block::COPPER_DOOR,
        Block::OXIDIZED_COPPER_DOOR,
        Block::WAXED_COPPER_DOOR,
        Block::OAK_TRAPDOOR,
        Block::IRON_TRAPDOOR,
        Block::COPPER_TRAPDOOR,
        Block::OAK_FENCE_GATE,
        Block::BAMBOO_FENCE_GATE,
    ]
    .into_iter()
    .enumerate()
    {
        let pos = BlockPos::new(34 + (case % 5) as i32 * 5, -59, 8 + (case / 5) as i32 * 8);
        let item = Item::from_name(block_type.name())
            .ok_or_else(|| anyhow::anyhow!("missing block item"))?;
        hand(bot, item).await?;
        click(bot, pos.below(1), Direction::Up).await?;
        bot.wait_for(
            "opening item placement",
            ferraloom_parity::Ticks::DEFAULT,
            |snapshot, _| {
                snapshot
                    .block(pos)
                    .is_some_and(|state| state.block() == block_type)
            },
        )
        .await?;
        hand(bot, Item::AIR).await?;
        let mut positions = vec![pos];
        if block(bot, pos)?.get(p::HALF).is_some() && block_type.name().ends_with("_door") {
            positions.push(pos.above(1));
        }
        for toggle in 0..2 {
            click(bot, pos, Direction::South).await?;
            step(bot, 1).await?;
            sample(
                bot,
                log,
                format!("opening.{case}.click.{toggle}"),
                &positions,
            )?;
            let manual = block_type != Block::IRON_DOOR && block_type != Block::IRON_TRAPDOOR;
            ensure!(
                block(bot, pos)?.get(p::OPEN) == Some(manual && toggle == 0),
                "wrong manual opening {block_type:?}"
            );
        }
        for (edge, on) in [("rise", true), ("fall", false)] {
            source(bot, pos.west(1), on).await?;
            timeline(bot, log, &format!("opening.{case}.{edge}"), &positions, 2).await?;
            for position in &positions {
                ensure!(block(bot, *position)?.get(p::OPEN) == Some(on));
                ensure!(block(bot, *position)?.get(p::POWERED) == Some(on));
            }
        }
    }
    Ok(())
}
async fn hook_line(bot: &mut Bot, start: BlockPos, distance: i32) -> Result<Vec<BlockPos>> {
    set(bot, start.west(1), Block::STONE).await?;
    set(bot, start.east(distance + 1), Block::STONE).await?;
    state(bot, start, "minecraft:tripwire_hook[facing=east]").await?;
    state(
        bot,
        start.east(distance),
        "minecraft:tripwire_hook[facing=west]",
    )
    .await?;
    for x in 1..distance {
        set(bot, start.east(x), Block::TRIPWIRE).await?;
    }
    Ok((0..=distance).map(|x| start.east(x)).collect())
}
async fn tripwires(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    let start = BlockPos::new(8, -59, 40);
    let positions = hook_line(bot, start, 10).await?;
    timeline(bot, log, "tripwire.attach", &positions, 11).await?;
    ensure!(block(bot, start)?.get(p::ATTACHED) == Some(true));
    bot.teleport([10.5, -59.0, 39.5]).await?;
    bot.move_to([10.5, -59.0, 40.5], true).await?;
    timeline(bot, log, "tripwire.contact", &positions, 2).await?;
    ensure!(
        block(bot, start)?.get(p::POWERED) == Some(true),
        "real player did not trigger the armed wire"
    );
    bot.move_to([10.5, -59.0, 39.5], true).await?;
    timeline(bot, log, "tripwire.release", &positions, 12).await?;
    ensure!(block(bot, start)?.get(p::POWERED) == Some(false));
    hand(bot, Item::SHEARS).await?;
    let cut = start.east(2);
    bot.teleport([f64::from(cut.x) + 0.5, -58.0, 42.5]).await?;
    bot.send(sb::Packet::PlayerAction(sb::PlayerActionPacket {
        action: play::PlayerAction::StartDestroyBlock,
        pos: cut,
        direction: play::DirectionByte(Direction::Up),
        sequence: VarInt(100),
    }))
    .await?;
    redstone_support::fence(bot).await?;
    bot.wait_for(
        "sheared wire removal",
        ferraloom_parity::Ticks::DEFAULT,
        |snapshot, _| snapshot.block(cut).is_some_and(|state| state.is_air()),
    )
    .await?;
    timeline(bot, log, "tripwire.disarm", &positions, 12).await?;
    ensure!(block(bot, start)?.get(p::POWERED) == Some(false));
    // A normal break must retain the armed source long enough to pulse hooks.
    set(bot, cut, Block::TRIPWIRE).await?;
    step(bot, 11).await?;
    set(bot, cut, Block::AIR).await?;
    timeline(bot, log, "tripwire.break", &positions, 12).await?;
    for distance in [2, 41, 42] {
        redstone_support::clear(bot, BlockPos::new(5, -59, 49), BlockPos::new(49, -59, 51)).await?;
        step(bot, 11).await?;
        let start = BlockPos::new(6, -59, 50);
        let positions = hook_line(bot, start, distance).await?;
        timeline(
            bot,
            log,
            &format!("tripwire.length.{distance}"),
            &positions,
            11,
        )
        .await?;
        ensure!(block(bot, start)?.get(p::ATTACHED) == Some(distance < 42));
    }
    Ok(())
}
async fn record(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::prepare(bot).await?;
    let mut log = ObservationLog::default();
    hoppers(bot, &mut log).await?;
    dispensers(bot, &mut log).await?;
    openings(bot, &mut log).await?;
    tripwires(bot, &mut log).await?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live Strict both-server hopper, dispenser, openings and tripwire scenarios"]
async fn parity_hopper_locks_dispense_quasi_and_openings() -> Result<()> {
    run(redstone_support::options(), RunMode::Both, record).await
}
