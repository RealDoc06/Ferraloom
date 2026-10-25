use ferraloom_data::{
    Item,
    registries::{custom_stat, stat_type},
};
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        Identifier,
        component::{DataComponentPatch, ItemStack},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::{BlockPos, Direction};

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
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
    bot.wait_for(command, Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains(expected)))).await?;
    bot.synchronize().await
}
async fn setup(session: &mut Session) -> Result<ferraloom_parity::BotId> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    bot.synchronize().await?;
    // GameModeCommand emits success only when the persisted mode changes.
    if bot.snapshot().game_mode != Some(play::GameType::Creative) {
        command(bot, "gamemode creative", "commands.gamemode.success").await?;
    }
    command(bot, "tick freeze", "commands.tick.status.frozen").await?;
    bot.teleport([8.5, 80.0, 10.5]).await?;
    Ok(player)
}
async fn stats(bot: &mut Bot) -> Result<cb::AwardStats> {
    bot.send(sb::Packet::ClientCommand(sb::ClientCommand {
        action: play::ClientCommandAction::RequestStats,
    }))
    .await?;
    let packet = bot
        .wait_for("statistics response", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::AwardStats(_))
        })
        .await?;
    let cb::Packet::AwardStats(packet) = packet.as_ref() else {
        unreachable!()
    };
    Ok(packet.clone())
}
fn chest_stat(packet: &cb::AwardStats) -> Option<i32> {
    packet
        .stats
        .iter()
        .find(|entry| {
            entry.stat_type.0 == i32::from(stat_type::Id::CUSTOM.protocol_id())
                && entry.value.0 == i32::from(custom_stat::Id::OPEN_CHEST.protocol_id())
        })
        .map(|entry| entry.count.0)
}
async fn chest_open_statistics(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    command(
        bot,
        "setblock 8 79 9 minecraft:chest",
        "commands.setblock.success",
    )
    .await?;
    stats(bot).await?;
    bot.use_item_on(
        play::InteractionHand::MainHand,
        play::BlockHitResult {
            block_pos: BlockPos::new(8, 79, 9),
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 1.0,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await?;
    let packet = bot
        .wait_for("opened chest", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::OpenScreen(_))
        })
        .await?;
    let cb::Packet::OpenScreen(open) = packet.as_ref() else {
        unreachable!()
    };
    let menu = open.container_id.0;
    bot.wait_for("chest contents", Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::ContainerSetContent(packet) if packet.container_id.0 == menu)).await?;
    let first = stats(bot).await?;
    ensure!(
        chest_stat(&first) == Some(1),
        "opening chest did not award exactly one statistic"
    );
    let second = stats(bot).await?;
    ensure!(
        chest_stat(&second).is_none(),
        "REQUEST_STATS did not clear chest's dirty entry"
    );
    let mut log = ObservationLog::default();
    log.record("open_chest", chest_stat(&first))?;
    log.record("dirty_cleared", chest_stat(&second).is_none())?;
    log.record("menu_slots", bot.snapshot().containers[&menu].slots.len())?;
    Ok(log)
}
async fn grant_crafting_table(bot: &mut Bot) -> Result<cb::UpdateAdvancements> {
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num: 36,
        item_stack: play::UntrustedItemStack(ItemStack::new(
            Item::CRAFTING_TABLE,
            1,
            DataComponentPatch::default(),
        )),
    }))
    .await?;
    let packet = bot.wait_for("inventory advancement", Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::UpdateAdvancements(packet) if packet.progress.iter().any(|progress| progress.id.as_str() == "minecraft:story/root" && progress.criteria.iter().any(|criterion| criterion.obtained.is_some())))).await?;
    let cb::Packet::UpdateAdvancements(packet) = packet.as_ref() else {
        unreachable!()
    };
    Ok(packet.clone())
}
async fn inventory_award_and_tab(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    let packet = grant_crafting_table(bot).await?;
    let root = packet
        .progress
        .iter()
        .find(|progress| progress.id.as_str() == "minecraft:story/root")
        .unwrap();
    ensure!(
        root.criteria
            .iter()
            .all(|criterion| criterion.obtained.is_some()),
        "root criterion incomplete"
    );
    let mut log = ObservationLog::default();
    log.record(
        "root_criteria",
        root.criteria
            .iter()
            .map(|criterion| (&criterion.name, criterion.obtained.is_some()))
            .collect::<Vec<_>>(),
    )?;
    let tab = Identifier::parse("minecraft:story/root")?;
    bot.send(sb::Packet::SeenAdvancements(
        sb::SeenAdvancements::OpenedTab(tab.clone()),
    ))
    .await?;
    let selected = bot.wait_for("selected advancement tab", Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::SelectAdvancementsTab(packet) if packet.tab.as_ref() == Some(&tab))).await?;
    let cb::Packet::SelectAdvancementsTab(selected) = selected.as_ref() else {
        unreachable!()
    };
    log.record(
        "selected_tab",
        selected.tab.as_ref().map(Identifier::as_str),
    )?;
    bot.send(sb::Packet::SeenAdvancements(
        sb::SeenAdvancements::ClosedScreen,
    ))
    .await?;
    bot.synchronize().await?;
    Ok(log)
}
async fn inventory_progress_survives_restart(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    grant_crafting_table(session.bot(player)?).await?;
    session.restart().await?;
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    bot.synchronize().await?;
    let state = bot.snapshot();
    let root = state
        .advancement_progress
        .get(&Identifier::parse("minecraft:story/root")?)
        .ok_or_else(|| anyhow::anyhow!("restored advancement progress missing"))?;
    ensure!(
        root.criteria
            .iter()
            .all(|criterion| criterion.obtained.is_some()),
        "saved root progress was lost"
    );
    let mut log = ObservationLog::default();
    log.record(
        "restored_criteria",
        root.criteria
            .iter()
            .map(|criterion| (&criterion.name, criterion.obtained.is_some()))
            .collect::<Vec<_>>(),
    )?;
    Ok(log)
}

async fn loaded_stats_and_tab(session: &mut Session) -> Result<ObservationLog> {
    let mut log = ObservationLog::default();
    for visit in ["join", "rejoin"] {
        let player = setup(session).await?;
        let bot = session.bot(player)?;
        let first = stats(bot).await?;
        ensure!(
            chest_stat(&first) == Some(7),
            "saved chest statistic lost on {visit}"
        );
        let second = stats(bot).await?;
        ensure!(
            chest_stat(&second).is_none(),
            "dirty entry repeated on {visit}"
        );
        let tab = Identifier::parse("minecraft:story/root")?;
        bot.send(sb::Packet::SeenAdvancements(
            sb::SeenAdvancements::OpenedTab(tab.clone()),
        ))
        .await?;
        let selected = bot.wait_for("loaded advancement tab", Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::SelectAdvancementsTab(packet) if packet.tab.as_ref() == Some(&tab))).await?;
        let cb::Packet::SelectAdvancementsTab(selected) = selected.as_ref() else {
            unreachable!()
        };
        log.record(format!("{visit}_stat"), chest_stat(&first))?;
        log.record(
            format!("{visit}_dirty_cleared"),
            chest_stat(&second).is_none(),
        )?;
        log.record(
            format!("{visit}_tab"),
            selected.tab.as_ref().map(Identifier::as_str),
        )?;
        if visit == "join" {
            session.restart().await?;
        }
    }
    Ok(log)
}

async fn location_interval(bot: &mut Bot) -> Result<()> {
    command(bot, "tick unfreeze", "commands.tick.status.running").await?;
    // Observe more than one native 20-tick interval without assuming the
    // player's tickCount was aligned to a boundary when the scenario joined.
    bot.wait_ticks(Ticks(40)).await?;
    command(bot, "tick freeze", "commands.tick.status.frozen").await
}

async fn leather_boots_location(session: &mut Session) -> Result<ObservationLog> {
    let player = setup(session).await?;
    let bot = session.bot(player)?;
    let target = Identifier::parse("minecraft:adventure/walk_on_powder_snow_with_leather_boots")?;
    let mut log = ObservationLog::default();
    for (label, block, boots, expected) in [
        ("wrong_boots", "powder_snow", Item::IRON_BOOTS, false),
        ("wrong_block", "stone", Item::LEATHER_BOOTS, false),
        (
            "leather_on_powder_snow",
            "powder_snow",
            Item::LEATHER_BOOTS,
            true,
        ),
    ] {
        // Replace the block before equipping the next boots, so no intermediate
        // state can satisfy the built-in predicate during connection ticks.
        command(
            bot,
            &format!("setblock 8 79 10 minecraft:{block}"),
            "commands.setblock.success",
        )
        .await?;
        bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
            slot_num: 8,
            item_stack: play::UntrustedItemStack(ItemStack::new(
                boots,
                1,
                DataComponentPatch::default(),
            )),
        }))
        .await?;
        bot.move_to([8.5, 80.0, 10.5], true).await?;
        location_interval(bot).await?;
        let state = bot.snapshot();
        let obtained = state
            .advancement_progress
            .get(&target)
            .is_some_and(|progress| {
                progress.criteria.iter().any(|criterion| {
                    criterion.name == "walk_on_powder_snow_with_leather_boots"
                        && criterion.obtained.is_some()
                })
            });
        ensure!(obtained == expected, "wrong LOCATION result for {label}");
        log.record(label, obtained)?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "launches disposable vanilla/Ferraloom servers"]
async fn location_awards_leather_boots_on_powder_snow() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::from_env()?, leather_boots_location).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "launches disposable vanilla/Ferraloom servers"]
async fn loaded_statistics_and_advancement_tab_survive_save() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let uuid = ferraloom_util::Uuid::offline_player("ParityBot");
    std::fs::create_dir_all(fixture.path().join("players/stats"))?;
    std::fs::create_dir_all(fixture.path().join("players/advancements"))?;
    std::fs::write(
        fixture
            .path()
            .join("players/stats")
            .join(format!("{uuid}.json")),
        serde_json::to_vec(
            &serde_json::json!({"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_chest":7}}}),
        )?,
    )?;
    std::fs::write(
        fixture
            .path()
            .join("players/advancements")
            .join(format!("{uuid}.json")),
        serde_json::to_vec(
            &serde_json::json!({"DataVersion":5023,"minecraft:story/root":{"criteria":{"crafting_table":"2026-10-18 00:00:00 +0000"},"done":true}}),
        )?,
    )?;
    let mut options = options();
    options.fixture = Some(fixture.path().to_path_buf());
    ferraloom_parity::run(options, RunMode::from_env()?, loaded_stats_and_tab).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "pending entity tick and BE listener/stat producer integration; run vanilla with FERRALOOM_PARITY_SIDE=vanilla"]
async fn opening_chest_awards_and_flushes_statistics() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::from_env()?, chest_open_statistics).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "pending entity tick and BE listener/stat producer integration; run vanilla with FERRALOOM_PARITY_SIDE=vanilla"]
async fn inventory_changed_awards_and_selects_advancement_tab() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::from_env()?, inventory_award_and_tab).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "pending entity tick and BE listener/stat producer integration; run vanilla with FERRALOOM_PARITY_SIDE=vanilla"]
async fn inventory_advancement_persists_across_player_save() -> Result<()> {
    ferraloom_parity::run(
        options(),
        RunMode::from_env()?,
        inventory_progress_survives_restart,
    )
    .await
}
