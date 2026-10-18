mod level_support;

use ferraloom_data::{Block, EntityType, Item};
use ferraloom_parity::{
    BotState, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{entity_data::EntityDataValue, play::clientbound as cb},
    run,
};
use ferraloom_util::BlockPos;
use level_support::*;

fn inventory_count(state: &BotState, item: Item) -> i32 {
    state
        .inventory
        .range(0..=35)
        .filter_map(|(_, stack)| stack.0.as_ref())
        .filter(|stack| stack.item == item)
        .map(|stack| stack.count)
        .sum()
}

fn dropped_items(state: &BotState, item: Item) -> Vec<(i32, [f64; 3], i32)> {
    state
        .entities
        .values()
        .filter(|entity| entity.entity_type == i32::from(EntityType::ITEM.protocol_id()))
        .filter_map(|entity| {
            let EntityDataValue::ItemStack(stack) = entity.data.get(&8)? else {
                return None;
            };
            let stack = stack.0.as_ref()?;
            (stack.item == item).then_some((entity.id, entity.position, stack.count))
        })
        .collect()
}

async fn falling_blocks_at_chunk_edge(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(bot, "gamemode spectator", "commands.gamemode.success.self").await?;
    bot.teleport([15.5, -56.0, 10.5]).await?;
    command(
        bot,
        "setblock 15 -60 8 minecraft:stone_slab[type=bottom]",
        "commands.setblock.success",
    )
    .await?;
    set(bot, BlockPos::new(15, -55, 8), Block::SAND).await?;
    set(bot, BlockPos::new(16, -55, 8), Block::GRAVEL).await?;
    step(bot, 40).await?;
    let slab = Block::STONE_SLAB.default_state();
    let expected = [
        (BlockPos::new(15, -60, 8), slab),
        (BlockPos::new(16, -60, 8), Block::GRAVEL.default_state()),
        (BlockPos::new(15, -55, 8), Block::AIR.default_state()),
        (BlockPos::new(16, -55, 8), Block::AIR.default_state()),
    ];
    expect_blocks(bot, &expected).await?;
    if dropped_items(&bot.snapshot(), Item::SAND).is_empty() {
        bot.wait_for(
            "sand dropped after slab landing",
            Ticks::DEFAULT,
            |state, _| !dropped_items(state, Item::SAND).is_empty(),
        )
        .await?;
    }
    let sand = dropped_items(&bot.snapshot(), Item::SAND);
    ensure!(
        sand.len() == 1 && sand[0].2 == 1,
        "slab landing did not drop exactly one sand: {sand:?}"
    );
    let mut log = ObservationLog::default();
    log.record(
        "landing.blocks",
        expected
            .iter()
            .map(|(pos, _)| Ok((pos.x, pos.y, block(bot, *pos)?.id())))
            .collect::<Result<Vec<_>>>()?,
    )?;
    log.record("landing.sand_drop_count", sand[0].2)?;
    Ok(log)
}

async fn item_drop_delay_and_pickup(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    bot.teleport([8.5, -60.0, 8.5]).await?;
    command(
        bot,
        "give @s minecraft:stone 3",
        "commands.give.success.single",
    )
    .await?;
    if inventory_count(&bot.snapshot(), Item::STONE) != 3 {
        bot.wait_for("given stone inventory", Ticks::DEFAULT, |state, _| {
            inventory_count(state, Item::STONE) == 3
        })
        .await?;
    }
    bot.look(0.0, 0.0).await?;
    bot.drop_item(false).await?;
    bot.wait_for(
        "one dropped stone and two retained",
        Ticks::DEFAULT,
        |state, _| {
            inventory_count(state, Item::STONE) == 2
                && dropped_items(state, Item::STONE)
                    .iter()
                    .any(|item| item.2 == 1)
        },
    )
    .await?;
    step(bot, 20).await?;
    let dropped = dropped_items(&bot.snapshot(), Item::STONE);
    ensure!(
        dropped.len() == 1 && dropped[0].2 == 1,
        "unexpected dropped stones: {dropped:?}"
    );
    let (item, position, _) = dropped[0];
    bot.teleport([position[0], -60.0, position[2]]).await?;
    step(bot, 19).await?;
    ensure!(
        inventory_count(&bot.snapshot(), Item::STONE) == 2,
        "own drop picked up before its 40-tick delay"
    );
    // Observe the take packet while stepping: a command helper would consume it
    // while waiting for its own feedback or the subsequent time-query reply.
    bot.command("tick step 2").await?;
    let packet = bot.wait_for("take dropped stone", Ticks::DEFAULT, |_, packet| {
        matches!(packet, cb::Packet::TakeItemEntity(take) if take.item_id.0 == item && take.amount.0 == 1)
    }).await?;
    let cb::Packet::TakeItemEntity(take) = packet.as_ref() else {
        unreachable!()
    };
    let taken = take.amount.0;
    if inventory_count(&bot.snapshot(), Item::STONE) != 3
        || bot.snapshot().entities.contains_key(&item)
    {
        bot.wait_for(
            "pickup inventory and removal",
            Ticks::DEFAULT,
            |state, _| {
                inventory_count(state, Item::STONE) == 3 && !state.entities.contains_key(&item)
            },
        )
        .await?;
    }
    let mut log = ObservationLog::default();
    log.record("before_delay.inventory", 2)?;
    log.record("pickup.take_count", taken)?;
    log.record(
        "pickup.inventory",
        inventory_count(&bot.snapshot(), Item::STONE),
    )?;
    log.record(
        "pickup.item_removed",
        !bot.snapshot().entities.contains_key(&item),
    )?;
    Ok(log)
}

async fn player_slab_stair_chunk_edge(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    for text in [
        "setblock 14 -60 8 minecraft:stone_slab[type=bottom]",
        "setblock 15 -60 8 minecraft:stone_stairs[facing=east,half=bottom,shape=straight]",
        "setblock 16 -60 8 minecraft:stone",
        "setblock 17 -60 8 minecraft:stone_slab[type=top]",
    ] {
        command(bot, text, "commands.setblock.success").await?;
    }
    bot.teleport([13.5, -60.0, 8.5]).await?;
    let teleports = bot.snapshot().teleports;
    let path = [
        [14.5, -59.5, 8.5],
        [15.25, -59.5, 8.5],
        [15.75, -59.0, 8.5],
        [16.5, -59.0, 8.5],
        [17.5, -59.0, 8.5],
        [18.5, -60.0, 8.5],
    ];
    for target in path {
        bot.move_to(target, true).await?;
        ensure!(
            bot.snapshot().teleports == teleports,
            "server corrected valid slab/stair movement at {target:?}"
        );
    }
    session.stop().await?;
    let saved = session.saved_world()?.player_position("ParityBot")?;
    ensure!(
        saved == path[path.len() - 1],
        "saved movement endpoint differs: {saved:?}"
    );
    let mut log = ObservationLog::default();
    log.record("movement.endpoint_bits", saved.map(f64::to_bits))?;
    log.record("movement.corrections", 0)?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires vanilla/JDK and integrated falling-block, item and tracking phases"]
async fn parity_falling_blocks_land_or_drop_at_a_chunk_edge() -> Result<()> {
    run(
        options(),
        RunMode::from_env()?,
        falling_blocks_at_chunk_edge,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires vanilla/JDK and integrated inventory, pickup and tracking phases"]
async fn parity_item_drop_respects_pickup_delay_and_returns_to_inventory() -> Result<()> {
    run(options(), RunMode::from_env()?, item_drop_delay_and_pickup).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires vanilla/JDK and integrated movement validation, commands and player saving"]
async fn parity_player_crosses_slabs_stairs_and_a_chunk_edge() -> Result<()> {
    run(
        options(),
        RunMode::from_env()?,
        player_slab_stair_chunk_edge,
    )
    .await
}
