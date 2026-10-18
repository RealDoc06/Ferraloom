use ferraloom_data::{EntityType, Item};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        codec::VarInt,
        component::{DataComponentPatch, HashedStack, ItemStack, Persistent, SignText},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, ChunkPos, Direction};

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:stone".into(), 4)],
            biome: "minecraft:plains".into(),
        },
        game_rules: [
            ("minecraft:random_tick_speed".into(), "0".into()),
            ("minecraft:spawn_mobs".into(), "false".into()),
        ]
        .into(),
        ..Default::default()
    }
}
async fn command(bot: &mut Bot, command: &str, expected: &str) -> Result<()> {
    bot.command(command).await?;
    bot.wait_for(command, Ticks::DEFAULT, |_, packet| matches!(packet,cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains(expected)))).await?;
    bot.synchronize().await
}
async fn game_time(bot: &mut Bot) -> Result<i64> {
    bot.command("time query gametime").await?;
    let packet=bot.wait_for("queried gametime",Ticks::DEFAULT,|_,packet| matches!(packet,cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text|text.contains("commands.time.query.gametime")))).await?;
    let cb::Packet::SystemChat(packet) = packet.as_ref() else {
        unreachable!()
    };
    let value: serde_json::Value = serde_json::from_str(&packet.content.to_json()?)?;
    let argument = &value["with"][0];
    argument
        .as_i64()
        .or_else(|| argument.as_str()?.parse().ok())
        .or_else(|| argument.get("text")?.as_str()?.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("missing numeric gametime: {value}"))
}
async fn step(bot: &mut Bot, ticks: i64) -> Result<()> {
    let initial = game_time(bot).await?;
    command(
        bot,
        &format!("tick step {ticks}"),
        "commands.tick.step.success",
    )
    .await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let now = game_time(bot).await?;
            ensure!(now <= initial + ticks, "tick step overshot");
            if now == initial + ticks {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    bot.synchronize().await
}
async fn setup(session: &mut Session) -> Result<ferraloom_parity::BotId> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    command(bot, "gamemode creative", "commands.gamemode.success").await?;
    command(bot, "tick freeze", "commands.tick.status.frozen").await?;
    bot.teleport([8.5, 80.0, 10.5]).await?;
    Ok(id)
}
async fn use_block(bot: &mut Bot, pos: BlockPos) -> Result<()> {
    bot.use_item_on(
        play::InteractionHand::MainHand,
        play::BlockHitResult {
            block_pos: pos,
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 1.0,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await
}
async fn open_chest(bot: &mut Bot, pos: BlockPos) -> Result<i32> {
    use_block(bot, pos).await?;
    let packet = bot
        .wait_for("chest menu", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::OpenScreen(_))
        })
        .await?;
    let cb::Packet::OpenScreen(open) = packet.as_ref() else {
        unreachable!()
    };
    let id = open.container_id.0;
    bot.wait_for("chest contents",Ticks::DEFAULT,|_,packet|matches!(packet,cb::Packet::ContainerSetContent(packet) if packet.container_id.0==id)).await?;
    Ok(id)
}
fn saved_entity(session: &Session, pos: BlockPos) -> Result<CompoundTag> {
    session
        .saved_world()?
        .block_entities("minecraft:overworld", ChunkPos::containing(pos))?
        .into_iter()
        .find(|tag| {
            tag.get_int("x") == Some(pos.x)
                && tag.get_int("y") == Some(pos.y)
                && tag.get_int("z") == Some(pos.z)
        })
        .ok_or_else(|| anyhow::anyhow!("missing saved block entity at {pos:?}"))
}
fn items(tag: &CompoundTag) -> Result<Vec<(i8, ItemStack)>> {
    tag.get_list("Items")
        .into_iter()
        .flat_map(|list| list.0.iter())
        .map(|tag| {
            let Tag::Compound(entry) = tag else {
                anyhow::bail!("non-compound container item")
            };
            Ok((
                entry.get_byte("Slot").unwrap_or(0),
                ItemStack::from_nbt(tag)?,
            ))
        })
        .collect()
}
async fn chest_store_break(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    let pos = BlockPos::new(8, 79, 9);
    command(
        bot,
        "setblock 8 79 9 minecraft:chest",
        "commands.setblock.success",
    )
    .await?;
    let stack = ItemStack::new(Item::STONE, 37, DataComponentPatch::default());
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num: 36,
        item_stack: play::UntrustedItemStack(stack.clone()),
    }))
    .await?;
    bot.synchronize().await?;
    let menu = open_chest(bot, pos).await?;
    bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
        container_id: VarInt(menu),
        state_id: VarInt(-1),
        slot_num: 54,
        button_num: 0,
        container_input: play::ContainerInput::QuickMove,
        changed_slots: play::ChangedSlots(Vec::new()),
        carried_item: HashedStack::default(),
    }))
    .await?;
    bot.wait_for("stored stack", Ticks::DEFAULT, |state, _| {
        state
            .containers
            .get(&menu)
            .is_some_and(|menu| menu.slots[0] == stack)
    })
    .await?;
    let mut log = ObservationLog::default();
    log.record(
        "stored",
        bot.snapshot().containers[&menu].slots[0].to_json()?,
    )?;
    bot.send(sb::Packet::ContainerClose(sb::ContainerClose {
        container_id: VarInt(menu),
    }))
    .await?;
    command(
        bot,
        "setblock 8 79 9 minecraft:air destroy",
        "commands.setblock.success",
    )
    .await?;
    bot.wait_for("container item drops", Ticks::DEFAULT, |state, _| {
        state
            .entities
            .values()
            .filter(|entity| entity.entity_type == EntityType::ITEM.protocol_id() as i32)
            .count()
            >= 2
    })
    .await?;
    bot.synchronize().await?;
    session.stop().await?;
    let mut drops = Vec::new();
    if let Some(entities) = session
        .saved_world()?
        .entities("minecraft:overworld", ChunkPos::new(0, 0))?
    {
        for entity in entities.entities.into_iter().flatten() {
            if entity
                .get_string("id")
                .is_some_and(|id| id.to_string() == "minecraft:item")
                && let Some(item) = entity.get_compound("Item")
            {
                drops.push(ItemStack::from_nbt(&Tag::Compound(item.clone()))?);
            }
        }
    }
    let stone = drops
        .iter()
        .filter_map(|stack| stack.0.as_ref())
        .filter(|stack| stack.item == Item::STONE)
        .map(|stack| stack.count)
        .sum::<i32>();
    ensure!(
        stone == 37,
        "container break must drop all stored stone, got {stone}"
    );
    log.record("dropped_stone", stone)?;
    log.record(
        "dropped_chest",
        drops
            .iter()
            .filter_map(|stack| stack.0.as_ref())
            .filter(|stack| stack.item == Item::CHEST)
            .map(|stack| stack.count)
            .sum::<i32>(),
    )?;
    Ok(log)
}

async fn smelt_furnace(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    command(
        bot,
        "setblock 8 79 9 minecraft:furnace",
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        "item replace block 8 79 9 container.0 with minecraft:iron_ore",
        "commands.item.block.replace.success.known_item",
    )
    .await?;
    command(
        bot,
        "item replace block 8 79 9 container.1 with minecraft:coal",
        "commands.item.block.replace.success.known_item",
    )
    .await?;
    step(bot, 200).await?;
    session.stop().await?;
    let entity = saved_entity(session, BlockPos::new(8, 79, 9))?;
    let slots = items(&entity)?;
    ensure!(
        slots.iter().any(|(slot, stack)| *slot == 2
            && stack
                .0
                .as_ref()
                .is_some_and(|stack| stack.item == Item::IRON_INGOT && stack.count == 1)),
        "furnace did not smelt iron"
    );
    let mut log = ObservationLog::default();
    log.record(
        "items",
        slots
            .into_iter()
            .map(|(slot, stack)| Ok((slot, stack.to_json()?)))
            .collect::<Result<Vec<_>>>()?,
    )?;
    for key in [
        "lit_time_remaining",
        "lit_total_time",
        "cooking_time_spent",
        "cooking_total_time",
    ] {
        log.record(key, entity.get_int_or(key, 0))?;
    }
    Ok(log)
}

async fn edit_sign(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    command(
        bot,
        "setblock 8 78 9 minecraft:stone",
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        "setblock 8 79 9 minecraft:oak_sign",
        "commands.setblock.success",
    )
    .await?;
    use_block(bot, BlockPos::new(8, 79, 9)).await?;
    let packet = bot
        .wait_for("sign editor", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::OpenSignEditor(_))
        })
        .await?;
    let cb::Packet::OpenSignEditor(editor) = packet.as_ref() else {
        unreachable!()
    };
    let lines = ["first", "§asecond", "héllo", ""];
    bot.send(sb::Packet::SignUpdate(sb::SignUpdate {
        pos: editor.pos,
        slot: editor.slot,
        lines: lines.map(protocol_utf8),
    }))
    .await?;
    bot.wait_for("sign text update",Ticks::DEFAULT,|_,packet|matches!(packet,cb::Packet::BlockEntityData(packet) if packet.pos==BlockPos::new(8,79,9))).await?;
    session.stop().await?;
    let entity = saved_entity(session, BlockPos::new(8, 79, 9))?;
    let key = if editor.slot == play::SignTextSlot::Front {
        "front_text"
    } else {
        "back_text"
    };
    let text = SignText::from_nbt(
        entity
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("missing edited sign text"))?,
    )?;
    ensure!(
        text.messages[1].contents
            == ferraloom_parity::protocol::chat::Contents::Text("second".into()),
        "formatting codes were not stripped"
    );
    let mut log = ObservationLog::default();
    log.record("edited_side", key)?;
    log.record(
        "lines",
        text.messages
            .iter()
            .map(|line| line.to_json())
            .collect::<std::result::Result<Vec<_>, _>>()?,
    )?;
    Ok(log)
}
fn protocol_utf8(text: &str) -> ferraloom_parity::protocol::common::Utf8<384> {
    ferraloom_parity::protocol::common::Utf8(text.into())
}

async fn structure_loot_chest(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    command(
        bot,
        "place template minecraft:village/plains/houses/plains_weaponsmith_1 0 80 0",
        "commands.place.template.success",
    )
    .await?;
    // The real vanilla template owns the chest and LootTable NBT at local 6,1,4.
    bot.teleport([6.5, 81.0, 6.5]).await?;
    command(
        bot,
        "data merge block 6 81 4 {LootTableSeed:12345L}",
        "commands.data.block.modified",
    )
    .await?;
    let menu = open_chest(bot, BlockPos::new(6, 81, 4)).await?;
    let slots = bot.snapshot().containers[&menu].slots[..27].to_vec();
    ensure!(
        slots.iter().any(|stack| !stack.is_empty()),
        "generated structure chest filled no loot"
    );
    let mut log = ObservationLog::default();
    log.record(
        "loot",
        slots
            .iter()
            .map(Persistent::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?,
    )?;
    bot.send(sb::Packet::ContainerClose(sb::ContainerClose {
        container_id: VarInt(menu),
    }))
    .await?;
    session.stop().await?;
    let entity = saved_entity(session, BlockPos::new(6, 81, 4))?;
    ensure!(
        !entity.contains_key("LootTable"),
        "opened chest retained its pending loot table"
    );
    log.record("loot_table_cleared", true)?;
    Ok(log)
}

macro_rules! pending_scenario {
    ($name:ident,$scenario:path,$pending:literal) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[ignore=$pending]
        async fn $name() -> Result<()> {
            ferraloom_parity::run(options(), RunMode::from_env()?, $scenario).await
        }
    };
}
pending_scenario!(
    open_store_break_chest,
    chest_store_break,
    "Ferraloom Strict pending advancement criteria, statistics and live block-drop integration; run vanilla with FERRALOOM_PARITY_SIDE=vanilla"
);
pending_scenario!(
    furnace_smelt_timeline,
    smelt_furnace,
    "pending integrated recipes, live container resources and item command path"
);
pending_scenario!(
    sign_edit_round_trip,
    edit_sign,
    "Ferraloom Strict pending default-block-use criteria and swing state; run vanilla with FERRALOOM_PARITY_SIDE=vanilla"
);
pending_scenario!(
    generated_structure_loot_chest,
    structure_loot_chest,
    "Ferraloom Strict pending structure placement, advancement criteria, statistics and live loot resources"
);
