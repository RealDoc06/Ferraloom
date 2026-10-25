//! Shared operations for the four redstone groups. Every group uses one live
//! Strict server per backend; observations are client state or native /data reads.
use crate::level_support::{self, block};
use ferraloom_data::{Block, Item};
use ferraloom_nbt::Tag;
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, Ticks, ensure,
    protocol::{
        common,
        component::ItemStack,
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, Direction};

pub fn options() -> LaunchOptions {
    let mut options = level_support::options();
    options.properties.extend([
        ("gamemode".into(), "creative".into()),
        ("view-distance".into(), "4".into()),
        ("simulation-distance".into(), "4".into()),
    ]);
    options
}
fn translated(value: &serde_json::Value, key: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields.get("translate").and_then(|value| value.as_str()) == Some(key)
                || fields.values().any(|value| translated(value, key))
        }
        serde_json::Value::Array(values) => values.iter().any(|value| translated(value, key)),
        _ => false,
    }
}
/// Command feedback is fenced by a second read-only command on the same server
/// thread. Native PingRequest is handled on the connection thread and cannot
/// fence block actions. Consume syntax-error context packets before the fence too.
async fn feedback(bot: &mut Bot, text: &str) -> Result<Vec<serde_json::Value>> {
    bot.command(text).await?;
    bot.command("time query time").await?;
    let mut messages = Vec::new();
    bot.wait_for(text, Ticks::DEFAULT, |_, packet| {
        if let cb::Packet::SystemChat(message) = packet
            && let Ok(json) = message.content.to_json()
            && let Ok(value) = serde_json::from_str(&json)
        {
            if translated(&value, "commands.time.query.absolute") {
                return true;
            }
            messages.push(value);
        }
        false
    })
    .await?;
    Ok(messages)
}
async fn checked_command(
    bot: &mut Bot,
    text: &str,
    expected: &str,
    unchanged: Option<&str>,
) -> Result<serde_json::Value> {
    let messages = feedback(bot, text).await?;
    messages
        .iter()
        .find(|message| {
            translated(message, expected) || unchanged.is_some_and(|key| translated(message, key))
        })
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{text}: expected {expected}, received {}",
                serde_json::Value::Array(messages)
            )
        })
}
pub async fn command(bot: &mut Bot, text: &str, expected: &str) -> Result<serde_json::Value> {
    checked_command(bot, text, expected, None).await
}
pub async fn fence(bot: &mut Bot) -> Result<()> {
    command(bot, "time query gametime", "commands.time.query.gametime").await?;
    Ok(())
}
pub async fn loaded(bot: &mut Bot, positions: &[BlockPos]) -> Result<()> {
    if positions
        .iter()
        .any(|pos| bot.snapshot().block(*pos).is_none())
    {
        bot.wait_for(
            "redstone fixture chunk columns",
            Ticks::DEFAULT,
            |snapshot, _| positions.iter().all(|pos| snapshot.block(*pos).is_some()),
        )
        .await?;
    }
    Ok(())
}
async fn type_received(bot: &mut Bot, pos: BlockPos, block: Block) -> Result<()> {
    if bot
        .snapshot()
        .block(pos)
        .is_none_or(|state| state.block() != block)
    {
        bot.wait_for("placed block broadcast", Ticks::DEFAULT, |snapshot, _| {
            snapshot
                .block(pos)
                .is_some_and(|state| state.block() == block)
        })
        .await?;
    }
    Ok(())
}
pub async fn prepare(bot: &mut Bot) -> Result<()> {
    command(bot, "tick freeze", "commands.tick.status.frozen").await?;
    command(bot, "tick query", "commands.tick.status.frozen").await?;
    // The shared bot joins with a two-chunk request. Server view distance alone
    // cannot keep this four-chunk-wide contraption tracked after camera moves.
    bot.send(sb::Packet::ClientInformation(
        common::serverbound::ClientInformation {
            language: common::FriendlyString("en_us".into()),
            view_distance: 4,
            chat_visibility: common::serverbound::ChatVisibility::Full,
            chat_colors: true,
            model_customisation: 127,
            main_hand: common::serverbound::HumanoidArm::Right,
            text_filtering_enabled: false,
            allows_listing: true,
            particle_status: common::serverbound::ParticleStatus::All,
        },
    ))
    .await?;
    fence(bot).await?;
    bot.teleport([16.5, -59.0, 16.5]).await?;
    let columns = (0..4)
        .flat_map(|x| (0..4).map(move |z| BlockPos::new(x * 16, -60, z * 16)))
        .collect::<Vec<_>>();
    loaded(bot, &columns).await?;
    command(
        bot,
        "fill 0 -60 0 63 -60 63 minecraft:stone strict",
        "commands.fill.success",
    )
    .await?;
    for pos in columns {
        type_received(bot, pos, Block::STONE).await?;
    }
    // Keep the player out of the observer/piston cells populated by the groups.
    bot.teleport([4.5, -59.0, 4.5]).await?;
    fence(bot).await
}
pub async fn state(bot: &mut Bot, pos: BlockPos, spec: &str) -> Result<()> {
    ensure!(
        !spec.contains('{'),
        "live inventory fixtures use canonical /data merge separately"
    );
    loaded(bot, &[pos]).await?;
    command(
        bot,
        &format!("setblock {} {} {} {spec}", pos.x, pos.y, pos.z),
        "commands.setblock.success",
    )
    .await?;
    let name = spec.split(['[', '{']).next().unwrap();
    let block =
        Block::from_name(name).ok_or_else(|| anyhow::anyhow!("unknown fixture block {name}"))?;
    type_received(bot, pos, block).await
}
pub async fn source(bot: &mut Bot, pos: BlockPos, powered: bool) -> Result<()> {
    set(
        bot,
        pos,
        if powered {
            Block::REDSTONE_BLOCK
        } else {
            Block::AIR
        },
    )
    .await
}
pub async fn game_time(bot: &mut Bot) -> Result<i64> {
    let response = command(bot, "time query gametime", "commands.time.query.gametime").await?;
    let arg = response
        .get("with")
        .and_then(|args| args.as_array())
        .and_then(|args| args.first())
        .ok_or_else(|| anyhow::anyhow!("missing game time: {response}"))?;
    arg.as_i64()
        .or_else(|| arg.as_str().and_then(|value| value.parse().ok()))
        .or_else(|| {
            arg.get("text")
                .and_then(|value| value.as_str())
                .and_then(|value| value.parse().ok())
        })
        .ok_or_else(|| anyhow::anyhow!("non-numeric game time: {response}"))
}
/// Each observation follows an actual completed world tick. Read the clock on
/// the server thread, after that tick's block and entity broadcasts, not a pong.
pub async fn step(bot: &mut Bot, ticks: i64) -> Result<()> {
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
            ensure!(
                now <= initial + ticks,
                "step overshot {initial}+{ticks}: {now}"
            );
            if now == initial + ticks {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await??;
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
    fence(bot).await
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
    fence(bot).await
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

/// Native no-change feedback is authoritative. Never infer whether an edit is
/// needed from a client snapshot that may precede the next chunk broadcast.
pub async fn set(bot: &mut Bot, pos: BlockPos, desired: Block) -> Result<()> {
    loaded(bot, &[pos]).await?;
    checked_command(
        bot,
        &format!("setblock {} {} {} {}", pos.x, pos.y, pos.z, desired.name()),
        "commands.setblock.success",
        Some("commands.setblock.failed"),
    )
    .await?;
    type_received(bot, pos, desired).await
}
pub async fn clear(bot: &mut Bot, min: BlockPos, max: BlockPos) -> Result<()> {
    loaded(bot, &[min, max]).await?;
    checked_command(
        bot,
        &format!(
            "fill {} {} {} {} {} {} minecraft:air",
            min.x, min.y, min.z, max.x, max.y, max.z
        ),
        "commands.fill.success",
        Some("commands.fill.failed"),
    )
    .await?;
    // A successful native fill, or its exact no-change error, establishes the
    // server state. Client observations below are made after completed ticks.
    Ok(())
}
