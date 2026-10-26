use super::{POS, expect, put, redstone_support, state, use_block};
use ferraloom_data::{Block, Item};
use ferraloom_parity::{
    Bot, ObservationLog, Result, Session, Ticks, ensure,
    protocol::{
        codec::VarInt,
        component::HashedStack,
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::Direction;

struct Workstation {
    block: Block,
    menu: &'static str,
    title: &'static str,
    slots: usize,
    stat: &'static str,
}

const WORKSTATIONS: [Workstation; 7] = [
    Workstation {
        block: Block::CRAFTING_TABLE,
        menu: "crafting",
        title: "container.crafting",
        slots: 46,
        stat: "interact_with_crafting_table",
    },
    Workstation {
        block: Block::ANVIL,
        menu: "anvil",
        title: "container.repair",
        slots: 39,
        stat: "interact_with_anvil",
    },
    Workstation {
        block: Block::LOOM,
        menu: "loom",
        title: "container.loom",
        slots: 40,
        stat: "interact_with_loom",
    },
    Workstation {
        block: Block::STONECUTTER,
        menu: "stonecutter",
        title: "container.stonecutter",
        slots: 38,
        stat: "interact_with_stonecutter",
    },
    Workstation {
        block: Block::GRINDSTONE,
        menu: "grindstone",
        title: "container.grindstone_title",
        slots: 39,
        stat: "interact_with_grindstone",
    },
    Workstation {
        block: Block::SMITHING_TABLE,
        menu: "smithing",
        title: "container.upgrade",
        slots: 40,
        stat: "interact_with_smithing_table",
    },
    Workstation {
        block: Block::CARTOGRAPHY_TABLE,
        menu: "cartography_table",
        title: "container.cartography_table",
        slots: 39,
        stat: "interact_with_cartography_table",
    },
];

fn registry_id(registry: &str, name: &str) -> i32 {
    let name = format!("minecraft:{name}");
    ferraloom_data::registry_entries(registry)
        .expect("native fixture registry")
        .iter()
        .position(|entry| *entry == name)
        .expect("native fixture entry") as i32
}

async fn open(
    bot: &mut Bot,
    log: &mut ObservationLog,
    label: &str,
    workstation: &Workstation,
) -> Result<i32> {
    bot.teleport([8.5, -59.0, 10.5]).await?;
    use_block(bot, POS, Direction::Up).await?;
    let screen = bot
        .wait_for("workstation OpenScreen", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::OpenScreen(_))
        })
        .await?;
    let cb::Packet::OpenScreen(screen) = screen.as_ref() else {
        unreachable!()
    };
    let title: serde_json::Value = serde_json::from_str(&screen.title.to_json()?)?;
    ensure!(
        title["translate"] == workstation.title,
        "wrong title: {title}"
    );
    ensure!(
        screen.menu_type.0 == registry_id("minecraft:menu", workstation.menu),
        "wrong menu kind"
    );
    let id = screen.container_id.0;
    log.record(format!("{label}.screen"), (id, screen.menu_type.0, title))?;
    let contents = bot.wait_for("initial workstation contents", Ticks::DEFAULT, |_, packet| {
        matches!(packet, cb::Packet::ContainerSetContent(contents) if contents.container_id.0 == id)
    }).await?;
    let cb::Packet::ContainerSetContent(contents) = contents.as_ref() else {
        unreachable!()
    };
    ensure!(
        contents.items.len() == workstation.slots,
        "wrong native slot count"
    );
    ensure!(
        contents.items.iter().all(|stack| stack.is_empty()) && contents.carried_item.is_empty(),
        "fixture starts with nonempty menu"
    );
    log.record(
        format!("{label}.contents"),
        (
            contents.state_id.0,
            contents.items.len(),
            contents.carried_item.is_empty(),
        ),
    )?;
    // A harmless invalid-slot click with a stale state ID asks native for a
    // full resync. Receiving this menu's reply proves it is the live receiver,
    // rather than an OpenScreen immediately invalidated by its bound access.
    bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
        container_id: VarInt(id),
        state_id: VarInt(-1),
        slot_num: -1,
        button_num: 0,
        container_input: play::ContainerInput::Pickup,
        changed_slots: play::ChangedSlots(Vec::new()),
        carried_item: HashedStack::default(),
    }))
    .await?;
    let synced = bot.wait_for("live workstation full resync", Ticks::DEFAULT, |_, packet| {
        matches!(packet, cb::Packet::ContainerSetContent(contents) if contents.container_id.0 == id)
    }).await?;
    let cb::Packet::ContainerSetContent(synced) = synced.as_ref() else {
        unreachable!()
    };
    ensure!(
        synced.items.len() == workstation.slots
            && synced.items.iter().all(|stack| stack.is_empty())
            && synced.carried_item.is_empty(),
        "empty-menu resync changed contents"
    );
    log.record(
        format!("{label}.resync"),
        (synced.state_id.0, synced.items.len()),
    )?;
    Ok(id)
}

async fn client_close(bot: &mut Bot, id: i32) -> Result<()> {
    bot.send(sb::Packet::ContainerClose(sb::ContainerClose {
        container_id: VarInt(id),
    }))
    .await?;
    redstone_support::fence(bot).await
}

async fn server_close(bot: &mut Bot, id: i32, command: &str) -> Result<()> {
    bot.command(command).await?;
    bot.wait_for("captured workstation access becomes invalid", Ticks::DEFAULT, |_, packet| {
        matches!(packet, cb::Packet::ContainerClose(close) if close.container_id.0 == id)
    }).await?;
    redstone_support::fence(bot).await
}

pub(super) async fn record(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::prepare(bot).await?;
    redstone_support::hand(bot, Item::AIR).await?;
    let mut log = ObservationLog::default();
    for workstation in &WORKSTATIONS {
        let placed = if workstation.block == Block::GRINDSTONE {
            state(workstation.block, &[("face", "floor"), ("facing", "north")])
        } else {
            workstation.block.default_state()
        };
        for reason in ["removed", "out_of_range"] {
            put(bot, POS, placed).await?;
            let label = format!("{}.{}", workstation.menu, reason);
            let menu = open(bot, &mut log, &label, workstation).await?;
            if reason == "removed" {
                server_close(bot, menu, "setblock 8 -59 8 minecraft:air").await?;
                expect(bot, POS, Block::AIR.default_state()).await?;
            } else {
                server_close(bot, menu, "tp @s 40.5 -59 8.5").await?;
            }
            log.record(format!("{label}.closed"), menu)?;
        }
    }
    // Actual BlockItem placement covers each damage variant and the four
    // native horizontal-direction -> clockwise facing results, over solid floor.
    for block in [Block::ANVIL, Block::CHIPPED_ANVIL, Block::DAMAGED_ANVIL] {
        for (yaw, facing) in [
            (0.0, "west"),
            (90.0, "north"),
            (180.0, "east"),
            (270.0, "south"),
        ] {
            put(bot, POS, Block::AIR.default_state()).await?;
            bot.teleport([8.5, -59.0, 10.5]).await?;
            bot.look(yaw, 45.0).await?;
            redstone_support::hand(bot, block.as_item()).await?;
            use_block(bot, POS.below(1), Direction::Up).await?;
            let placed = state(block, &[("facing", facing)]);
            expect(bot, POS, placed).await?;
            redstone_support::hand(bot, Item::AIR).await?;
            let label = format!("placement.{}.{}", block.name(), facing);
            log.record(
                format!("{label}.state"),
                super::level_support::block(bot, POS)?.id(),
            )?;
            let menu = open(bot, &mut log, &label, &WORKSTATIONS[1]).await?;
            client_close(bot, menu).await?;
        }
    }
    bot.send(sb::Packet::ClientCommand(sb::ClientCommand {
        action: play::ClientCommandAction::RequestStats,
    }))
    .await?;
    let stats = bot
        .wait_for(
            "workstation interaction statistics",
            Ticks::DEFAULT,
            |_, packet| matches!(packet, cb::Packet::AwardStats(_)),
        )
        .await?;
    let cb::Packet::AwardStats(stats) = stats.as_ref() else {
        unreachable!()
    };
    for workstation in &WORKSTATIONS {
        let stat = registry_id("minecraft:custom_stat", workstation.stat);
        let count = stats
            .stats
            .iter()
            .find(|value| {
                value.stat_type.0 == registry_id("minecraft:stat_type", "custom")
                    && value.value.0 == stat
            })
            .map_or(0, |value| value.count.0);
        let expected = if workstation.block == Block::ANVIL {
            14
        } else {
            2
        };
        ensure!(
            count == expected,
            "{} awarded {count} rather than {expected}",
            workstation.stat
        );
        log.record(format!("stat.{}", workstation.stat), count)?;
    }
    Ok(log)
}
