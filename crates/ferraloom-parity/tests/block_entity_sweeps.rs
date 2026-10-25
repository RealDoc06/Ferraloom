//! One boot per backend covers the live Strict furnace/storage group.
#[allow(dead_code)] // This target uses only the clock/command subset of the fixture.
mod level_support;

use anyhow::Context;
use ferraloom_data::{Block, Item, properties as p};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        chat::{Argument, Component, Contents},
        component::{ItemStack, Persistent},
        play::clientbound as cb,
    },
};
use ferraloom_util::{BlockPos, ChunkPos};
use level_support::{command, step};

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:stone".into(), 4)],
            biome: "minecraft:plains".into(),
        },
        properties: [("gamemode".into(), "creative".into())].into(),
        ..level_support::options()
    }
}

// Compound key order is not NBT value semantics. Preserve every tag's numeric
// kind and float bits while canonicalizing only compound key iteration.
fn nbt(tag: &Tag) -> Result<serde_json::Value> {
    use serde_json::json;
    Ok(match tag {
        Tag::End => json!(["end"]),
        Tag::Byte(v) => json!(["byte", v]),
        Tag::Short(v) => json!(["short", v]),
        Tag::Int(v) => json!(["int", v]),
        Tag::Long(v) => json!(["long", v]),
        Tag::Float(v) => json!(["float", v.to_bits()]),
        Tag::Double(v) => json!(["double", v.to_bits()]),
        Tag::ByteArray(v) => json!(["byte_array", v]),
        Tag::IntArray(v) => json!(["int_array", v]),
        Tag::LongArray(v) => json!(["long_array", v]),
        Tag::String(v) => json!(["string", v.to_string_checked()?]),
        Tag::List(v) => json!(["list", v.iter().map(nbt).collect::<Result<Vec<_>>>()?]),
        Tag::Compound(v) => {
            let values = v
                .iter()
                .map(|(key, value)| Ok((key.to_string_checked()?, nbt(value)?)))
                .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
            json!(["compound", values])
        }
    })
}
fn text(component: &Component) -> Result<String> {
    let Contents::Text(value) = &component.contents else {
        anyhow::bail!(
            "NBT pretty printer emitted nonliteral contents: {:?}",
            component.contents
        )
    };
    let mut value = value.clone();
    for extra in &component.extra {
        value.push_str(&text(extra)?);
    }
    Ok(value)
}
async fn block_nbt(bot: &mut Bot, pos: BlockPos) -> Result<CompoundTag> {
    let query = format!("data get block {} {} {}", pos.x, pos.y, pos.z);
    bot.command(&query).await?;
    let response = bot.wait_for(&query, Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|value| value.contains("commands.data.block.query") || value.contains("commands.generic.failed") || value.contains("commands.data.block.invalid")))).await?;
    let cb::Packet::SystemChat(response) = response.as_ref() else {
        unreachable!()
    };
    let Contents::Translatable { key, args, .. } = &response.content.contents else {
        anyhow::bail!("unexpected block NBT response: {:?}", response.content)
    };
    ensure!(
        key == "commands.data.block.query",
        "{query}: {:?}",
        response.content
    );
    let raw = match args.get(3).context("block query omitted NBT argument")? {
        Argument::Component(value) => text(value)?,
        Argument::String(value) => value.clone(),
        other => anyhow::bail!("unexpected block NBT argument: {other:?}"),
    };
    Ok(ferraloom_nbt::parse_compound(&raw)?)
}
async fn set(bot: &mut Bot, pos: BlockPos, block: Block) -> Result<()> {
    command(
        bot,
        &format!("setblock {} {} {} {}", pos.x, pos.y, pos.z, block.name()),
        "commands.setblock.success",
    )
    .await?;
    Ok(())
}
fn slot(tag: &CompoundTag, slot: i8) -> Result<ItemStack> {
    let Some(entry) = tag
        .get_list("Items")
        .into_iter()
        .flat_map(|items| items.iter())
        .find(|value| {
            value
                .as_compound()
                .is_some_and(|value| value.get_byte_or("Slot", 0) == slot)
        })
    else {
        return Ok(ItemStack::default());
    };
    Ok(ItemStack::from_nbt(entry)?)
}

const FURNACES: [(Block, Item, Item, Item); 5] = [
    (Block::FURNACE, Item::IRON_ORE, Item::COAL, Item::IRON_INGOT),
    (
        Block::SMOKER,
        Item::PORKCHOP,
        Item::COAL,
        Item::COOKED_PORKCHOP,
    ),
    (
        Block::BLAST_FURNACE,
        Item::RAW_IRON,
        Item::COAL,
        Item::IRON_INGOT,
    ),
    (Block::FURNACE, Item::SAND, Item::LAVA_BUCKET, Item::GLASS),
    (
        Block::FURNACE,
        Item::IRON_ORE,
        Item::BAMBOO,
        Item::IRON_INGOT,
    ),
];
fn furnace_pos(index: usize) -> BlockPos {
    BlockPos::new(5 + index as i32 * 2, 79, 8)
}

async fn furnace_and_storage(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    bot.synchronize().await?;
    level_support::freeze(bot).await?;
    set(bot, BlockPos::new(8, 79, 13), Block::STONE).await?;
    bot.teleport([8.5, 80.0, 13.5]).await?;
    for (index, &(block, input, fuel, _)) in FURNACES.iter().enumerate() {
        let pos = furnace_pos(index);
        set(bot, pos, block).await?;
        // A native storage load, not a substitute for Container#setItem. A
        // positive saved total lets serverTick recompute the real recipe/fuel
        // duration when ignition starts. Input replacement remains a separate
        // pending scenario until /item replace is ported.
        command(bot, &format!(
            "data merge block {} {} {} {{Items:[{{Slot:0b,id:'{}',count:{}}},{{Slot:1b,id:'{}',count:1}}],cooking_total_time:200}}",
            pos.x, pos.y, pos.z, input.name(), if index < 3 { 2 } else { 1 }, fuel.name()
        ), "commands.data.block.modified").await?;
    }
    let mut log = ObservationLog::default();
    let mut previous = 0;
    for elapsed in [0, 1, 50, 51, 100, 200, 400] {
        if elapsed != 0 {
            step(bot, elapsed - previous).await?;
        }
        for (index, _) in FURNACES.iter().enumerate() {
            let pos = furnace_pos(index);
            let tag = block_nbt(bot, pos).await?;
            ensure!(
                tag.get_list("Items").is_some(),
                "missing furnace items at {elapsed}: {tag:?}"
            );
            if elapsed == 1 {
                ensure!(
                    tag.get_int_or("lit_time_remaining", 0) > 0
                        && tag.get_int_or("cooking_time_spent", 0) == 1,
                    "furnace did not start at tick1: {tag:?}"
                );
                if index == 3 {
                    ensure!(
                        slot(&tag, 1)?.item() == Item::BUCKET,
                        "lava fuel did not leave its bucket"
                    );
                }
            }
            if index == 4 && elapsed >= 100 {
                ensure!(
                    slot(&tag, 2)?.is_empty(),
                    "one bamboo unexpectedly completed smelting"
                );
            }
            log.record(
                format!("furnace.{index}.tick.{elapsed}"),
                nbt(&Tag::Compound(tag))?,
            )?;
        }
        previous = elapsed;
    }
    for (index, &(_, _, _, expected)) in
        FURNACES.iter().enumerate().filter(|(index, _)| *index != 4)
    {
        let tag = block_nbt(bot, furnace_pos(index)).await?;
        let output = slot(&tag, 2)?;
        ensure!(
            output.item() == expected
                && output
                    .0
                    .as_ref()
                    .is_some_and(|stack| stack.count == if index < 3 { 2 } else { 1 }),
            "wrong final smelting output at {index}: {tag:?}"
        );
    }
    for (index, block) in [Block::CHEST, Block::FURNACE].into_iter().enumerate() {
        let pos = BlockPos::new(7 + index as i32 * 2, 79, 11);
        set(bot, pos, block).await?;
        for count in [0, 100] {
            command(bot, &format!("data merge block {} {} {} {{Items:[{{Slot:0b,id:'minecraft:stone',count:{count},components:{{'minecraft:custom_name':'partial'}}}},{{Slot:2b,id:'minecraft:diamond',count:3}}]}}", pos.x, pos.y, pos.z), "commands.data.block.modified").await?;
            let tag = block_nbt(bot, pos).await?;
            ensure!(
                slot(&tag, 0)?
                    .0
                    .as_ref()
                    .is_some_and(|stack| stack.item == Item::STONE && stack.count == 1),
                "storage read did not retain the native count partial: {tag:?}"
            );
            ensure!(
                slot(&tag, 2)?
                    .0
                    .as_ref()
                    .is_some_and(|stack| stack.item == Item::DIAMOND && stack.count == 3),
                "partial read changed another slot"
            );
            log.record(
                format!("storage.{index}.count.{count}"),
                nbt(&Tag::Compound(tag))?,
            )?;
        }
    }
    // One save/stop after every case, with the same five live tickers and two
    // storage receivers. No backend restarts or per-case boots are hidden here.
    session.stop().await?;
    let saved = session.saved_world()?;
    for (index, _) in FURNACES.iter().enumerate() {
        let pos = furnace_pos(index);
        let tag = saved
            .block_entities("minecraft:overworld", ChunkPos::containing(pos))?
            .into_iter()
            .find(|tag| {
                tag.get_int("x") == Some(pos.x)
                    && tag.get_int("y") == Some(pos.y)
                    && tag.get_int("z") == Some(pos.z)
            })
            .context("missing saved furnace")?;
        log.record(format!("saved.furnace.{index}"), nbt(&Tag::Compound(tag))?)?;
        let state = saved.block("minecraft:overworld", pos)?;
        let lit = state
            .properties
            .as_ref()
            .and_then(|properties| properties.get(&"lit".into()));
        let lit = match lit
            .map(|value| value.to_string_checked())
            .transpose()?
            .as_deref()
        {
            Some("true") => true,
            Some("false") => false,
            None => FURNACES[index].0.default_state().get(p::LIT).unwrap(),
            other => anyhow::bail!("invalid saved lit property: {other:?}"),
        };
        log.record(format!("saved.furnace.{index}.lit"), lit)?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live Strict both-server sweep; run explicitly in the builder parity job"]
async fn strict_furnace_fuel_timeline_and_container_storage() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::Both, furnace_and_storage).await
}
