mod level_support;

use ferraloom_data::{Block, Item, properties as p, registries::stat_type};
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{
        codec::VarInt,
        component::{
            BlockItemStateProperties, DataComponent, DataComponentPatch, HashedStack, ItemStack,
            JavaF32, UseCooldown,
        },
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, Direction};

async fn mode(bot: &mut Bot, creative: bool) -> Result<()> {
    let target = if creative {
        play::GameType::Creative
    } else {
        play::GameType::Survival
    };
    if bot.snapshot().game_mode != Some(target) {
        level_support::command(
            bot,
            if creative {
                "gamemode creative"
            } else {
                "gamemode survival"
            },
            "commands.gamemode.success",
        )
        .await?;
    }
    Ok(())
}

async fn resync_hand(bot: &mut Bot) -> Result<ItemStack> {
    // Vanilla suppresses predicted slot deltas. A mismatched click requests
    // its normal complete inventory synchronization, without changing a slot.
    bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
        container_id: VarInt(0),
        state_id: VarInt(-1),
        slot_num: -1,
        button_num: 0,
        container_input: play::ContainerInput::Pickup,
        changed_slots: play::ChangedSlots(Vec::new()),
        carried_item: HashedStack::default(),
    }))
    .await?;
    bot.wait_for(
        "placement inventory resync",
        Ticks::DEFAULT,
        |_, packet| matches!(packet, cb::Packet::ContainerSetContent(p) if p.container_id.0 == 0),
    )
    .await?;
    Ok(bot
        .snapshot()
        .inventory
        .get(&0)
        .cloned()
        .unwrap_or_default())
}

async fn used_stat(bot: &mut Bot, item: Item) -> Result<i32> {
    bot.send(sb::Packet::ClientCommand(sb::ClientCommand {
        action: play::ClientCommandAction::RequestStats,
    }))
    .await?;
    let packet = bot
        .wait_for("item use statistics", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::AwardStats(_))
        })
        .await?;
    let cb::Packet::AwardStats(stats) = packet.as_ref() else {
        unreachable!()
    };
    Ok(stats
        .stats
        .iter()
        .find(|entry| {
            entry.stat_type.0 == i32::from(stat_type::Id::USED.protocol_id())
                && entry.value.0 == i32::from(item.protocol_id())
        })
        .map_or(0, |entry| entry.count.0))
}

async fn build_blocks(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    level_support::freeze(bot).await?;
    bot.teleport([8.5, 80.0, 12.5]).await?;
    let mut log = ObservationLog::default();
    let cases = [
        (Item::STONE, Block::STONE.default_state(), false, false),
        (Item::DIRT, Block::DIRT.default_state(), false, false),
        (
            Item::RED_MUSHROOM_BLOCK,
            Block::RED_MUSHROOM_BLOCK
                .default_state()
                .with(p::NORTH, false)
                .unwrap(),
            false,
            false,
        ),
        (Item::STONE, Block::STONE.default_state(), true, true),
    ];
    for (case, (item, expected, creative, cooldown)) in cases.into_iter().enumerate() {
        let pos = BlockPos::new(7 + case as i32, 80, 9);
        level_support::set(bot, pos.below(1), Block::STONE).await?;
        mode(bot, true).await?;
        let mut components = DataComponentPatch::default();
        if item == Item::RED_MUSHROOM_BLOCK {
            components
                .added
                .push(DataComponent::BlockState(BlockItemStateProperties(vec![(
                    "north".into(),
                    "false".into(),
                )])));
        }
        if cooldown {
            components
                .added
                .push(DataComponent::UseCooldown(UseCooldown {
                    seconds: JavaF32(1.25),
                    cooldown_group: Some("test:placement".parse()?),
                }));
        }
        bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
            slot_num: 36,
            item_stack: play::UntrustedItemStack(ItemStack::new(item, 3, components)),
        }))
        .await?;
        bot.synchronize().await?;
        mode(bot, creative).await?;
        bot.use_item_on(
            play::InteractionHand::MainHand,
            play::BlockHitResult {
                block_pos: pos.below(1),
                direction: Direction::Up,
                offset_x: 0.5,
                offset_y: 1.0,
                offset_z: 0.5,
                inside: false,
                world_border_hit: false,
            },
        )
        .await?;
        if cooldown {
            let packet = bot.wait_for("placement use cooldown", Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::Cooldown(p) if p.cooldown_group.as_str() == "test:placement")).await?;
            let cb::Packet::Cooldown(packet) = packet.as_ref() else {
                unreachable!()
            };
            ensure!(packet.duration.0 == 25, "wrong placement cooldown");
            log.record("creative.cooldown", packet.duration.0)?;
        }
        level_support::expect_blocks(bot, &[(pos, expected)]).await?;
        let held = resync_hand(bot).await?;
        ensure!(
            held.item() == item && held.count() == if creative { 3 } else { 2 },
            "wrong placed-stack consumption: {held:?}"
        );
        log.record(
            format!("place.{case}.state"),
            level_support::block(bot, pos)?.id(),
        )?;
        log.record(format!("place.{case}.count"), held.count())?;
        let used = used_stat(bot, item).await?;
        let expected_uses = if case == 3 { 2 } else { 1 };
        ensure!(used == expected_uses, "wrong ITEM_USED statistic: {used}");
        log.record(format!("place.{case}.used"), used)?;
    }
    level_support::step(bot, 1).await?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opt-in both-server ordinary BlockItem placement parity"]
async fn ordinary_block_items_place_consume_and_award_native_use_statistics() -> Result<()> {
    ferraloom_parity::run(level_support::options(), RunMode::from_env()?, build_blocks).await
}
