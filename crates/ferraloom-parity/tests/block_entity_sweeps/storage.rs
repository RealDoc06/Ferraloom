//! Storage interactions share one Strict server lifetime per backend.
use super::{block_nbt, level_support, nbt, options, set};
use anyhow::Context;
use ferraloom_data::{Block, EntityType, Item, properties as p};
use ferraloom_nbt::Tag;
use ferraloom_parity::{
    Bot, BotState, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{
        codec::VarInt,
        component::{DataComponentPatch, ItemStack, Persistent},
        entity_data::EntityDataValue,
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, Direction};
use serde_json::{Value, json};
use std::sync::Arc;

async fn interact(bot: &mut Bot, pos: BlockPos) -> Result<()> {
    bot.use_item_on_and_await_ack(
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

/// Capture at each packet wait rather than draining a separate output queue.
/// Sound seed/pitch use the world's unsynchronized startup RNG: compare sound,
/// position, source and volume, and assert the native pitch range on both sides.
struct Capture {
    positions: Vec<BlockPos>,
    packets: Vec<Value>,
    nonce: i64,
}
impl Capture {
    fn new(positions: Vec<BlockPos>) -> Self {
        Self {
            positions,
            packets: Vec::new(),
            nonce: i64::MIN,
        }
    }
    fn observe(&mut self, packet: &cb::Packet) {
        match packet {
            cb::Packet::BlockEvent(event) if self.positions.contains(&event.pos) => {
                self.packets.push(json!([
                    "event",
                    [event.pos.x, event.pos.y, event.pos.z],
                    event.block.0,
                    event.b0,
                    event.b1
                ]));
            }
            cb::Packet::Sound(sound) => {
                let name = sound.sound.to_json().expect("decoded sound holder");
                if name.as_str().is_some_and(|name| {
                    name.contains("block.chest.")
                        || name.contains("block.barrel.")
                        || name.contains("block.copper_chest.")
                }) {
                    // Native nextFloat()*0.1f+0.9f can round its largest
                    // draw to exactly 1.0f, despite nextFloat's excluded end.
                    assert!(
                        (0.9..=1.0).contains(&sound.pitch),
                        "native container sound pitch: {}",
                        sound.pitch
                    );
                    assert_eq!(sound.volume, 0.5);
                    self.packets.push(json!([
                        "sound",
                        name,
                        [sound.x, sound.y, sound.z],
                        format!("{:?}", sound.source),
                        sound.volume
                    ]));
                }
            }
            cb::Packet::OpenScreen(menu) => self.packets.push(json!([
                "open",
                menu.container_id.0,
                menu.menu_type.0,
                menu.title.to_json().expect("decoded title")
            ])),
            cb::Packet::ContainerClose(menu) => {
                self.packets.push(json!(["close", menu.container_id.0]))
            }
            _ => {}
        }
    }
    async fn wait(
        &mut self,
        bot: &mut Bot,
        label: &str,
        mut done: impl FnMut(&BotState, &cb::Packet) -> bool,
    ) -> Result<Arc<cb::Packet>> {
        bot.wait_for(label, Ticks::DEFAULT, |state, packet| {
            self.observe(packet);
            done(state, packet)
        })
        .await
    }
    async fn command(&mut self, bot: &mut Bot, command: &str, expected: &str) -> Result<Value> {
        bot.command(command).await?;
        let packet = self.wait(bot, command, |_, packet| matches!(packet, cb::Packet::SystemChat(chat) if chat.content.to_json().is_ok_and(|value| value.contains(expected) || value.contains("commands.generic.") || value.contains("ferraloom.command.pending")))).await?;
        let cb::Packet::SystemChat(chat) = packet.as_ref() else {
            unreachable!()
        };
        let value = chat.content.to_json()?;
        ensure!(value.contains(expected), "{command}: {value}");
        Ok(serde_json::from_str(&value)?)
    }
    async fn time(&mut self, bot: &mut Bot) -> Result<i64> {
        let response = self
            .command(bot, "time query gametime", "commands.time.query.gametime")
            .await?;
        let value = &response["with"][0];
        value
            .as_i64()
            .or_else(|| value.as_str()?.parse().ok())
            .or_else(|| value.get("text")?.as_str()?.parse().ok())
            .context("gametime argument")
    }
    async fn step(&mut self, bot: &mut Bot) -> Result<()> {
        let before = self.time(bot).await?;
        self.command(bot, "tick step 1", "commands.tick.step.success")
            .await?;
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let now = self.time(bot).await?;
                ensure!(now <= before + 1, "tick step overshot");
                if now == before + 1 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        self.barrier(bot).await
    }
    async fn barrier(&mut self, bot: &mut Bot) -> Result<()> {
        self.nonce += 1;
        let nonce = self.nonce;
        bot.send(sb::Packet::PingRequest(sb::PingRequest { time: nonce }))
            .await?;
        self.wait(
            bot,
            "captured packet barrier",
            |_, packet| matches!(packet, cb::Packet::PongResponse(pong) if pong.time == nonce),
        )
        .await?;
        Ok(())
    }
    async fn open(&mut self, bot: &mut Bot, pos: BlockPos) -> Result<i32> {
        interact(bot, pos).await?;
        let packet = self
            .wait(bot, "storage menu open", |_, packet| {
                matches!(packet, cb::Packet::OpenScreen(_))
            })
            .await?;
        let cb::Packet::OpenScreen(menu) = packet.as_ref() else {
            unreachable!()
        };
        let id = menu.container_id.0;
        self.wait(bot, "storage menu contents", |_, packet| matches!(packet, cb::Packet::ContainerSetContent(content) if content.container_id.0 == id)).await?;
        self.step(bot).await?;
        ensure!(
            bot.snapshot().containers.contains_key(&id),
            "menu {id} closed after stepping at {pos:?}; packets={:?}",
            self.packets
        );
        Ok(id)
    }
    async fn close(&mut self, bot: &mut Bot, id: i32) -> Result<()> {
        bot.send(sb::Packet::ContainerClose(sb::ContainerClose {
            container_id: VarInt(id),
        }))
        .await?;
        self.step(bot).await
    }
}

fn contents(bot: &Bot, menu: i32, count: usize) -> Result<Vec<Value>> {
    bot.snapshot()
        .containers
        .get(&menu)
        .context("opened menu")?
        .slots[..count]
        .iter()
        .map(|stack| {
            if stack.is_empty() {
                Ok(json!({}))
            } else {
                stack.to_json()
            }
        })
        .collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}

fn dropped_count(state: &BotState, item: Item) -> i32 {
    state
        .entities
        .values()
        .filter(|entity| entity.entity_type == i32::from(EntityType::ITEM.protocol_id()))
        .filter_map(|entity| match entity.data.get(&8) {
            Some(EntityDataValue::ItemStack(stack)) if stack.item() == item => {
                stack.0.as_ref().map(|stack| stack.count)
            }
            _ => None,
        })
        .sum()
}

async fn storage_interactions(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    level_support::freeze(bot).await?;
    // Player travel is server-authoritative in 26.3 and players keep ticking
    // while the world is frozen. Every interaction position needs a floor.
    for z in [11, 12, 13] {
        set(bot, BlockPos::new(8, 79, z), Block::STONE).await?;
    }
    bot.teleport([8.5, 80.0, 13.5]).await?;
    let mut log = ObservationLog::default();
    for (case, block, double) in [
        ("chest", Block::CHEST, false),
        ("double", Block::CHEST, true),
        ("barrel", Block::BARREL, false),
    ] {
        let pos = BlockPos::new(8, 79, 9);
        set(bot, pos, block).await?;
        let mut positions = vec![pos];
        if double {
            level_support::command(
                bot,
                "setblock 8 79 9 chest[facing=north,type=left]",
                "commands.setblock.success",
            )
            .await?;
            level_support::command(
                bot,
                "setblock 9 79 9 chest[facing=north,type=right]",
                "commands.setblock.success",
            )
            .await?;
            level_support::command(
                bot,
                "data merge block 9 79 9 {Items:[{Slot:0b,id:'minecraft:diamond',count:3}]}",
                "commands.data.block.modified",
            )
            .await?;
            positions.push(pos.east(1));
        }
        level_support::command(
            bot,
            "data merge block 8 79 9 {Items:[{Slot:0b,id:'minecraft:stone',count:7}]}",
            "commands.data.block.modified",
        )
        .await?;
        bot.teleport([8.5, 80.0, 11.5]).await?;
        if case == "chest" {
            bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
                slot_num: 36,
                item_stack: play::UntrustedItemStack(ItemStack::new(
                    Item::STONE,
                    37,
                    DataComponentPatch::default(),
                )),
            }))
            .await?;
        }
        bot.synchronize().await?;
        if block == Block::CHEST {
            set(bot, pos.above(1), Block::STONE).await?;
            bot.synchronize().await?;
            let mut blocked = Capture::new(positions.clone());
            interact(bot, pos).await?;
            blocked.barrier(bot).await?;
            ensure!(
                !blocked.packets.iter().any(|packet| packet[0] == "open"),
                "blocked lid opened a menu"
            );
            log.record(format!("{case}.blocked-lid"), blocked.packets)?;
            set(bot, pos.above(1), Block::AIR).await?;
            bot.synchronize().await?;
        }
        let mut capture = Capture::new(positions);
        let menu = capture.open(bot, pos).await?;
        let count = if double { 54 } else { 27 };
        let slots = bot
            .snapshot()
            .containers
            .get(&menu)
            .with_context(|| format!("{case}: opened storage menu {menu} disappeared"))?
            .slots
            .clone();
        ensure!(slots.len() == count + 36, "wrong storage menu size");
        let expected = if double { Item::DIAMOND } else { Item::STONE };
        ensure!(slots[0].item() == expected, "wrong first container half");
        if double {
            ensure!(
                slots[27].item() == Item::STONE,
                "RIGHT must precede LEFT in a double chest"
            );
        }
        log.record(format!("{case}.contents"), contents(bot, menu, count)?)?;
        if block == Block::BARREL {
            ensure!(
                bot.snapshot()
                    .block(pos)
                    .and_then(|state| state.get(p::OPEN))
                    == Some(true),
                "barrel did not open"
            );
        }
        // Insert the held stack into the single chest, merging with its seven
        // stored stones; the other cases quick-move their first slot out.
        // The same native slot receiver is visible in the BE after the click.
        // A deliberately stale state id requests the canonical full resync.
        bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
            container_id: VarInt(menu),
            state_id: VarInt(-1),
            slot_num: if case == "chest" {
                (count + 27) as i16
            } else {
                0
            },
            button_num: 0,
            container_input: play::ContainerInput::QuickMove,
            changed_slots: play::ChangedSlots(Vec::new()),
            carried_item: Default::default(),
        }))
        .await?;
        capture
            .wait(bot, "storage quick move", |state, _| {
                state.containers.get(&menu).is_some_and(|menu| {
                    if case == "chest" {
                        menu.slots[0]
                            .0
                            .as_ref()
                            .is_some_and(|stack| stack.item == Item::STONE && stack.count == 44)
                    } else {
                        menu.slots[0].is_empty()
                    }
                })
            })
            .await?;
        capture.barrier(bot).await?;
        log.record(
            format!("{case}.after-quick-move"),
            contents(bot, menu, count)?,
        )?;
        capture.close(bot, menu).await?;
        if block == Block::BARREL {
            ensure!(
                bot.snapshot()
                    .block(pos)
                    .and_then(|state| state.get(p::OPEN))
                    == Some(false),
                "barrel did not close"
            );
        } else {
            for pos in &capture.positions {
                for openers in [1, 0] {
                    ensure!(
                        capture.packets.iter().any(|event| event[0] == "event"
                            && event[1] == json!([pos.x, pos.y, pos.z])
                            && event[3] == 1
                            && event[4] == openers),
                        "missing chest opener event {pos:?}/{openers}"
                    );
                }
            }
        }
        log.record(format!("{case}.opener-packets"), &capture.packets)?;
        log.record(
            format!("{case}.saved"),
            nbt(&Tag::Compound(block_nbt(bot, pos).await?))?,
        )?;
        // Removing after close is part of the live lifecycle; old scheduled
        // opener checks then encounter AIR rather than a replacement receiver.
        if case == "chest" {
            bot.teleport([8.5, 80.0, 13.5]).await?;
            level_support::command(
                bot,
                "setblock 8 79 9 air destroy",
                "commands.setblock.success",
            )
            .await?;
            bot.wait_for(
                "tracked chest and full stored contents",
                Ticks::DEFAULT,
                |state, _| {
                    dropped_count(state, Item::STONE) == 44
                        && dropped_count(state, Item::CHEST) == 1
                },
            )
            .await?;
            bot.synchronize().await?;
            log.record(
                "chest.dropped-stone",
                dropped_count(&bot.snapshot(), Item::STONE),
            )?;
            log.record(
                "chest.dropped-block",
                dropped_count(&bot.snapshot(), Item::CHEST),
            )?;
        } else {
            set(bot, pos, Block::AIR).await?;
        }
        if double {
            set(bot, pos.east(1), Block::AIR).await?;
        }
    }
    // One explicitly seeded native structure chest in the same server lifetime.
    level_support::command(
        bot,
        "place template minecraft:village/plains/houses/plains_weaponsmith_1 0 80 0",
        "commands.place.template.success",
    )
    .await?;
    let loot_pos = BlockPos::new(6, 81, 4);
    bot.teleport([6.5, 81.0, 6.5]).await?;
    level_support::command(
        bot,
        "data merge block 6 81 4 {LootTableSeed:12345L}",
        "commands.data.block.modified",
    )
    .await?;
    let pending = block_nbt(bot, loot_pos).await?;
    ensure!(
        pending.contains_key("LootTable"),
        "structure did not install lazy loot"
    );
    let mut capture = Capture::new(vec![loot_pos]);
    let menu = capture.open(bot, loot_pos).await?;
    let filled = contents(bot, menu, 27)?;
    ensure!(
        bot.snapshot().containers[&menu].slots[..27]
            .iter()
            .any(|stack| !stack.is_empty()),
        "structure loot chest filled no items"
    );
    log.record("structure.loot", filled)?;
    capture.close(bot, menu).await?;
    let loaded = block_nbt(bot, loot_pos).await?;
    ensure!(
        !loaded.contains_key("LootTable"),
        "opened chest retained pending loot"
    );
    log.record("structure.saved", nbt(&Tag::Compound(loaded))?)?;
    log.record("structure.opener-packets", capture.packets)?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live Strict storage group; one boot per backend"]
async fn strict_chest_barrel_double_inventory_openers_and_structure_loot() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::Both, storage_interactions).await
}
