//! All sign interactions in this group share one Strict boot per backend.
use super::{block_nbt, level_support, nbt, options, set};
use ferraloom_data::Block;
use ferraloom_nbt::Tag;
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{
        common::Utf8,
        component::{Persistent, SignText},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, Direction};

const POS: BlockPos = BlockPos::new(8, 79, 9);

async fn use_sign(bot: &mut Bot) -> Result<()> {
    bot.use_item_on(
        play::InteractionHand::MainHand,
        play::BlockHitResult {
            block_pos: POS,
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 0.5,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await
}

async fn open_editor(bot: &mut Bot, expected: play::SignTextSlot) -> Result<()> {
    bot.synchronize().await?;
    use_sign(bot).await?;
    let mut update_before_editor = false;
    let packet = bot
        .wait_for("sign update then editor", Ticks::DEFAULT, |_, packet| {
            if matches!(packet, cb::Packet::BlockUpdate(update) if update.pos == POS) {
                update_before_editor = true;
            }
            matches!(packet, cb::Packet::OpenSignEditor(editor) if editor.pos == POS)
        })
        .await?;
    let cb::Packet::OpenSignEditor(editor) = packet.as_ref() else {
        unreachable!()
    };
    ensure!(update_before_editor, "editor preceded native BlockUpdate");
    ensure!(editor.slot == expected, "wrong front/back editor slot");
    bot.synchronize().await
}

async fn send_text(bot: &mut Bot, slot: play::SignTextSlot, lines: [&str; 4]) -> Result<()> {
    bot.send(sb::Packet::SignUpdate(sb::SignUpdate {
        pos: POS,
        slot,
        lines: lines.map(|line| Utf8(line.into())),
    }))
    .await
}

async fn sign_edits(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let intruder = session.join("SignPeer").await?;
    let bot = session.bot(player)?;
    level_support::freeze(bot).await?;
    set(bot, BlockPos::new(8, 79, 14), Block::STONE).await?;
    bot.teleport([8.5, 80.0, 14.5]).await?;
    // Supports remain in place across the four cases; native /setblock rejects
    // replacing a block with its identical state.
    for support in [POS.below(1), POS.above(1), POS.south(1), POS.east(1)] {
        set(bot, support, Block::STONE).await?;
    }
    let mut log = ObservationLog::default();
    for (index, (block, north_front)) in [
        (Block::OAK_SIGN, false),
        (Block::OAK_WALL_SIGN, true),
        (Block::OAK_HANGING_SIGN, false),
        (Block::OAK_WALL_HANGING_SIGN, true),
    ]
    .into_iter()
    .enumerate()
    {
        let bot = session.bot(player)?;
        set(bot, POS, block).await?;
        for front in [true, false] {
            let slot = if front {
                play::SignTextSlot::Front
            } else {
                play::SignTextSlot::Back
            };
            let bot = session.bot(player)?;
            let north = front == north_front;
            bot.teleport([8.5, 80.0, if north { 6.5 } else { 12.5 }])
                .await?;
            open_editor(bot, slot).await?;
            let before = block_nbt(bot, POS).await?;
            // Another loaded client cannot consume the editor session, even
            // though handleSignUpdate intentionally has no packet distance gate.
            let peer = session.bot(intruder)?;
            send_text(peer, slot, ["intruder", "", "", ""]).await?;
            peer.synchronize().await?;
            let bot = session.bot(player)?;
            ensure!(
                block_nbt(bot, POS).await? == before,
                "non-editor changed sign text"
            );
            let lines = if front {
                ["front", "§asecond", "héllo", ""]
            } else {
                ["back", "§bsecond", "世界", ""]
            };
            send_text(bot, slot, lines).await?;
            let packet = bot.wait_for("edited sign update tag", Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::BlockEntityData(update) if update.pos == POS)).await?;
            let cb::Packet::BlockEntityData(update) = packet.as_ref() else {
                unreachable!()
            };
            log.record(
                format!("sign.{index}.{front}.packet"),
                nbt(&Tag::Compound(update.tag.0.clone()))?,
            )?;
            let tag = block_nbt(bot, POS).await?;
            let key = if front { "front_text" } else { "back_text" };
            let text = SignText::from_nbt(tag.get(key).expect("native saved side"))?;
            for (line, expected) in text.messages.iter().zip([lines[0], "second", lines[2], ""]) {
                ensure!(
                    line.contents
                        == ferraloom_parity::protocol::chat::Contents::Text(expected.into()),
                    "formatting stripping changed a sign line"
                );
            }
            log.record(
                format!("sign.{index}.{front}.saved"),
                nbt(&Tag::Compound(tag.clone()))?,
            )?;
            // A successful edit clears the editor; replay cannot change it.
            send_text(bot, slot, ["replay", "", "", ""]).await?;
            bot.synchronize().await?;
            ensure!(
                block_nbt(bot, POS).await? == tag,
                "completed editor accepted replay"
            );
        }
        let bot = session.bot(player)?;
        let front_z = if north_front { 6.5 } else { 12.5 };
        bot.teleport([8.5, 80.0, front_z]).await?;
        open_editor(bot, play::SignTextSlot::Front).await?;
        level_support::command(
            bot,
            "data merge block 8 79 9 {is_waxed:1b}",
            "commands.data.block.modified",
        )
        .await?;
        let waxed = block_nbt(bot, POS).await?;
        send_text(bot, play::SignTextSlot::Front, ["waxed", "", "", ""]).await?;
        bot.synchronize().await?;
        ensure!(
            block_nbt(bot, POS).await? == waxed,
            "waxed sign accepted edit"
        );
        log.record(format!("sign.{index}.waxed"), nbt(&Tag::Compound(waxed))?)?;
        level_support::command(
            bot,
            "data merge block 8 79 9 {is_waxed:0b}",
            "commands.data.block.modified",
        )
        .await?;
        // Reopening installs a fresh editor; the real BE ticker clears it when
        // the player moves beyond the live interaction range plus four blocks,
        // while the chunk remains ticking.
        open_editor(bot, play::SignTextSlot::Front).await?;
        bot.teleport([8.5, 80.0, 23.5]).await?;
        level_support::step(bot, 1).await?;
        let distant = block_nbt(bot, POS).await?;
        send_text(bot, play::SignTextSlot::Front, ["distant", "", "", ""]).await?;
        bot.synchronize().await?;
        ensure!(
            block_nbt(bot, POS).await? == distant,
            "expired distant editor accepted edit"
        );
        log.record(
            format!("sign.{index}.distance-expired"),
            nbt(&Tag::Compound(distant))?,
        )?;
        set(bot, POS, Block::AIR).await?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live Strict both-server sign group; one boot per backend"]
async fn strict_four_sign_classes_front_back_editor_wax_and_tick() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::Both, sign_edits).await
}
