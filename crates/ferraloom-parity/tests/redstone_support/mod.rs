//! Shared operations for the four redstone groups. Every group uses one live
//! Strict server per backend; observations are client state or native /data reads.
use crate::level_support::{self, block, command, step};
use ferraloom_data::{Block, Item};
use ferraloom_nbt::Tag;
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result,
    protocol::{
        component::ItemStack,
        play::{self, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, Direction};

pub fn options() -> LaunchOptions {
    let mut options = level_support::options();
    options
        .properties
        .insert("gamemode".into(), "creative".into());
    options
}
pub async fn prepare(bot: &mut Bot) -> Result<()> {
    level_support::freeze(bot).await?;
    command(
        bot,
        "fill 0 -60 0 63 -60 63 minecraft:stone strict",
        "commands.fill.success",
    )
    .await?;
    bot.teleport([4.5, -59.0, 4.5]).await
}
pub async fn state(bot: &mut Bot, pos: BlockPos, spec: &str) -> Result<()> {
    command(
        bot,
        &format!("setblock {} {} {} {spec}", pos.x, pos.y, pos.z),
        "commands.setblock.success",
    )
    .await?;
    Ok(())
}
pub async fn source(bot: &mut Bot, pos: BlockPos, powered: bool) -> Result<()> {
    let desired = if powered {
        Block::REDSTONE_BLOCK
    } else {
        Block::AIR
    };
    if block(bot, pos)?.block() != desired {
        level_support::set(bot, pos, desired).await?;
    }
    Ok(())
}
pub fn sample(
    bot: &Bot,
    log: &mut ObservationLog,
    label: impl Into<String>,
    positions: &[BlockPos],
) -> Result<()> {
    log.record(
        label,
        positions
            .iter()
            .map(|pos| block(bot, *pos).map(|state| state.id()))
            .collect::<Result<Vec<_>>>()?,
    )
}
pub async fn timeline(
    bot: &mut Bot,
    log: &mut ObservationLog,
    label: &str,
    positions: &[BlockPos],
    ticks: i64,
) -> Result<()> {
    sample(bot, log, format!("{label}.0"), positions)?;
    for tick in 1..=ticks {
        step(bot, 1).await?;
        sample(bot, log, format!("{label}.{tick}"), positions)?;
    }
    Ok(())
}
pub async fn hand(bot: &mut Bot, item: Item) -> Result<()> {
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num: 36,
        item_stack: play::UntrustedItemStack(if item == Item::AIR {
            ItemStack::default()
        } else {
            ItemStack::new(item, 1, Default::default())
        }),
    }))
    .await?;
    bot.send(sb::Packet::SetCarriedItem(sb::SetCarriedItem { slot: 0 }))
        .await?;
    bot.synchronize().await
}
pub async fn click(bot: &mut Bot, pos: BlockPos, face: Direction) -> Result<()> {
    bot.teleport([
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 1.0,
        f64::from(pos.z) + 2.5,
    ])
    .await?;
    bot.use_item_on(
        play::InteractionHand::MainHand,
        play::BlockHitResult {
            block_pos: pos,
            direction: face,
            offset_x: 0.5,
            offset_y: 0.5,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await?;
    bot.synchronize().await
}
fn text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values.iter().map(text).collect(),
        serde_json::Value::Object(value) => {
            let mut result = value.get("text").map(text).unwrap_or_default();
            if let Some(extra) = value.get("extra") {
                result.push_str(&text(extra));
            }
            result
        }
        value => value.to_string(),
    }
}
/// Read native NBT feedback and discard presentation styling, never tag values.
pub async fn data(bot: &mut Bot, pos: BlockPos, path: &str) -> Result<Tag> {
    let response = command(
        bot,
        &format!("data get block {} {} {} {path}", pos.x, pos.y, pos.z),
        "commands.data.block.query",
    )
    .await?;
    let component = response
        .get("with")
        .and_then(|args| args.as_array())
        .and_then(|args| args.last())
        .ok_or_else(|| anyhow::anyhow!("data response omitted value: {response}"))?;
    ferraloom_nbt::parse(&text(component)).map_err(Into::into)
}
pub async fn inventory(bot: &mut Bot, pos: BlockPos) -> Result<Vec<(i8, String, i32)>> {
    let Tag::List(items) = data(bot, pos, "Items").await? else {
        anyhow::bail!("container Items is not a list")
    };
    let mut result = Vec::new();
    for item in items.0 {
        let Tag::Compound(item) = item else {
            anyhow::bail!("non-compound container item")
        };
        result.push((
            item.get_byte("Slot")
                .ok_or_else(|| anyhow::anyhow!("missing Slot"))?,
            item.get_string("id")
                .ok_or_else(|| anyhow::anyhow!("missing id"))?
                .to_string_checked()?,
            item.get_int("count")
                .ok_or_else(|| anyhow::anyhow!("missing count"))?,
        ));
    }
    result.sort();
    Ok(result)
}
pub async fn merge(bot: &mut Bot, pos: BlockPos, nbt: &str) -> Result<()> {
    command(
        bot,
        &format!("data merge block {} {} {} {nbt}", pos.x, pos.y, pos.z),
        "commands.data.block.modified",
    )
    .await?;
    Ok(())
}

/// Skip only an identical native state, avoiding /setblock's no-change error.
pub async fn set(bot: &mut Bot, pos: BlockPos, desired: Block) -> Result<()> {
    if block(bot, pos)? != desired.default_state() {
        level_support::set(bot, pos, desired).await?;
    }
    Ok(())
}
pub async fn clear(bot: &mut Bot, min: BlockPos, max: BlockPos) -> Result<()> {
    let mut occupied = false;
    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                occupied |= !block(bot, BlockPos::new(x, y, z))?.is_air();
            }
        }
    }
    if occupied {
        command(
            bot,
            &format!(
                "fill {} {} {} {} {} {} minecraft:air",
                min.x, min.y, min.z, max.x, max.y, max.z
            ),
            "commands.fill.success",
        )
        .await?;
    }
    Ok(())
}
