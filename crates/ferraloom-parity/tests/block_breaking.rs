mod level_support;

use anyhow::Context;
use ferraloom_data::{Block, registries::stat_type};
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{
        codec::VarInt,
        component::{ItemStack, Persistent},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use level_support::{command, expect_blocks, freeze, options, set};

const SURVIVAL: BlockPos = BlockPos::new(8, -60, 8);
const CREATIVE: BlockPos = BlockPos::new(10, -60, 8);

async fn action(bot: &Bot, action: play::PlayerAction, pos: BlockPos, sequence: i32) -> Result<()> {
    bot.send(sb::Packet::PlayerAction(sb::PlayerActionPacket {
        action,
        pos,
        direction: play::DirectionByte(Direction::Up),
        sequence: VarInt(sequence),
    }))
    .await
}
async fn stats(bot: &mut Bot) -> Result<cb::AwardStats> {
    bot.send(sb::Packet::ClientCommand(sb::ClientCommand {
        action: play::ClientCommandAction::RequestStats,
    }))
    .await?;
    let packet = bot
        .wait_for("mining statistics", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::AwardStats(_))
        })
        .await?;
    let cb::Packet::AwardStats(packet) = packet.as_ref() else {
        unreachable!()
    };
    Ok(packet.clone())
}
fn mined(packet: &cb::AwardStats, block: Block) -> i32 {
    packet
        .stats
        .iter()
        .find(|stat| {
            stat.stat_type.0 == i32::from(stat_type::Id::MINED.protocol_id())
                && stat.value.0 == i32::from(block.protocol_id())
        })
        .map_or(0, |stat| stat.count.0)
}

/// Real START/STOP packets, the native miner-excluded crack audience, stat
/// responses and persisted ItemEntity data from one Strict server lifetime.
async fn survival_and_creative(session: &mut Session) -> Result<ObservationLog> {
    let miner = session.join("ParityBot").await?;
    let observer = session.join("BreakObserver").await?;
    {
        let bot = session.bot(miner)?;
        freeze(bot).await?;
        if bot.snapshot().game_mode != Some(play::GameType::Survival) {
            command(bot, "gamemode survival", "commands.gamemode.success").await?;
        }
        bot.teleport([8.5, -60.0, 10.5]).await?;
        bot.move_to([8.5, -60.0, 10.5], true).await?;
        ensure!(
            bot.snapshot().inventory.values().all(ItemStack::is_empty),
            "mining fixture requires an empty hand and inventory"
        );
        set(bot, SURVIVAL, Block::DIRT).await?;
        set(bot, CREATIVE, Block::STONE).await?;
        expect_blocks(
            bot,
            &[
                (SURVIVAL, Block::DIRT.default_state()),
                (CREATIVE, Block::STONE.default_state()),
            ],
        )
        .await?;
        ensure!(
            mined(&stats(bot).await?, Block::DIRT) == 0,
            "fresh mining stat"
        );
    }
    {
        let bot = session.bot(observer)?;
        command(bot, "gamemode spectator", "commands.gamemode.success").await?;
        bot.teleport([6.5, -60.0, 10.5]).await?;
        bot.synchronize().await?;
    }
    let source = session
        .bot(miner)?
        .snapshot()
        .entity_id
        .context("miner network ID")?;
    action(
        session.bot(miner)?,
        play::PlayerAction::StartDestroyBlock,
        SURVIVAL,
        1,
    )
    .await?;
    session.bot(observer)?
        .wait_for("initial crack stage", Ticks::DEFAULT, |_, packet| {
            matches!(packet,cb::Packet::BlockDestruction(p) if p.id.0==source && p.pos==SURVIVAL && p.progress==0)
        }).await?;
    let mut stages = Vec::new();
    session
        .bot(observer)?
        .wait_for("survival crack progress", Ticks::DEFAULT, |_, packet| {
            if let cb::Packet::BlockDestruction(p) = packet
                && p.id.0 == source
                && p.pos == SURVIVAL
                && p.progress > 0
                && p.progress < 255
            {
                stages.push(p.progress);
                p.progress >= 7
            } else {
                false
            }
        })
        .await?;
    ensure!(
        stages == [1, 2, 3, 4, 5, 6, 7],
        "native dirt crack stages: {stages:?}"
    );
    action(
        session.bot(miner)?,
        play::PlayerAction::StopDestroyBlock,
        SURVIVAL,
        2,
    )
    .await?;
    session.bot(observer)?
        .wait_for("stop clears cracks", Ticks::DEFAULT, |_, packet| {
            matches!(packet,cb::Packet::BlockDestruction(p) if p.id.0==source && p.pos==SURVIVAL && p.progress==255)
        }).await?;
    {
        let bot = session.bot(miner)?;
        expect_blocks(bot, &[(SURVIVAL, Block::AIR.default_state())]).await?;
        ensure!(
            mined(&stats(bot).await?, Block::DIRT) == 1,
            "one mined dirt statistic"
        );
        command(bot, "gamemode creative", "commands.gamemode.success").await?;
        action(bot, play::PlayerAction::StartDestroyBlock, CREATIVE, 3).await?;
        expect_blocks(bot, &[(CREATIVE, Block::AIR.default_state())]).await?;
        ensure!(
            mined(&stats(bot).await?, Block::STONE) == 0,
            "creative must not award mined stone"
        );
        ensure!(
            bot.snapshot().inventory.values().all(ItemStack::is_empty),
            "dropped dirt was not collected"
        );
        bot.synchronize().await?;
    }
    let mut log = ObservationLog::default();
    log.record("survival.cracks", stages)?;
    log.record("survival.cleared", true)?;
    log.record("survival.mined_dirt", 1)?;
    log.record("creative.instant_air", true)?;
    log.record("creative.mined_stone", 0)?;
    session.stop().await?;
    let saved = session.saved_world()?;
    let entities = saved
        .entities("minecraft:overworld", ChunkPos::ZERO)?
        .context("saved mining entity chunk")?;
    let mut drops = Vec::new();
    for tag in entities.entities.context("saved mining entity list")? {
        if tag
            .get_string("id")
            .is_none_or(|id| id.to_string_lossy() != "minecraft:item")
        {
            continue;
        }
        let item = tag.get_compound("Item").context("saved item stack")?;
        let stack = ItemStack::from_nbt(&ferraloom_nbt::Tag::Compound(item.clone()))?;
        let stack = stack.0.context("nonempty block drop")?;
        drops.push((
            stack.item.name().to_owned(),
            stack.count,
            tag.get_short("PickupDelay").context("pickup delay")?,
        ));
    }
    drops.sort();
    ensure!(
        drops == [(String::from("minecraft:dirt"), 1, 10)],
        "one synchronous dirt drop, no creative stone drop: {drops:?}"
    );
    log.record("saved.block_drops", drops)?;
    ensure!(
        saved
            .block("minecraft:overworld", SURVIVAL)?
            .name
            .context("saved survival block ID")?
            .to_string_lossy()
            == "minecraft:air",
        "saved survival block"
    );
    ensure!(
        saved
            .block("minecraft:overworld", CREATIVE)?
            .name
            .context("saved creative block ID")?
            .to_string_lossy()
            == "minecraft:air",
        "saved creative block"
    );
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires real 26.3 Java server and integrated Strict block breaking"]
async fn survival_and_creative_break_blocks() -> Result<()> {
    let mut options = options();
    options.operators.push("BreakObserver".into());
    ferraloom_parity::run(options, RunMode::from_env()?, survival_and_creative).await
}
