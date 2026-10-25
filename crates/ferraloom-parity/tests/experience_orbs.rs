#[expect(
    dead_code,
    reason = "The orb scenario uses only non-stepping shared fixtures."
)]
mod level_support;

use anyhow::Context;
use ferraloom_data::{EntityType, Item, registries::data_component_type::Id as ComponentType};
use ferraloom_parity::{
    Bot, BotState, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{
        component::{DataComponent, Persistent},
        play::clientbound as cb,
    },
    run,
};
use level_support::{command, freeze};

fn pickaxe_damage(state: &BotState) -> Option<i32> {
    let stack = state.inventory.get(&0)?;
    if stack.item() != Item::DIAMOND_PICKAXE {
        return None;
    }
    match stack.component(ComponentType::DAMAGE).ok()?? {
        DataComponent::Damage(value) => Some(value.0),
        _ => None,
    }
}

async fn collect(
    bot: &mut Bot,
    label: &str,
    count: i32,
    previous_xp: i32,
    expected_xp: i32,
    mend: bool,
    log: &mut ObservationLog,
) -> Result<()> {
    bot.synchronize().await?;
    let player = bot.snapshot().entity_id.context("player network id")?;
    bot.command(&format!(
        "summon minecraft:experience_orb 8.5 -60 8.5 {{Value:7s,Count:{count}}}"
    ))
    .await?;
    let mut orb = None;
    let mut takes = 0;
    let mut removed = false;
    let mut experience = None;
    let mut summoned = false;
    let mut order = Vec::new();
    bot.wait_for(
        "plain or Mending orb pickup",
        Ticks::DEFAULT,
        |state, packet| {
            match packet {
                cb::Packet::AddEntity(entity)
                    if entity.entity_type.0
                        == i32::from(EntityType::EXPERIENCE_ORB.protocol_id()) =>
                {
                    orb = Some(entity.id.0);
                }
                cb::Packet::TakeItemEntity(event)
                    if Some(event.item_id.0) == orb && event.player_id.0 == player =>
                {
                    takes += event.amount.0;
                    order.push("take".to_owned());
                }
                cb::Packet::RemoveEntities(event)
                    if orb.is_some_and(|orb| {
                        event.entity_ids.iter().any(|entity| entity.0 == orb)
                    }) =>
                {
                    removed = true;
                    order.push("remove".to_owned());
                }
                cb::Packet::SetExperience(event) if event.total_experience.0 > previous_xp => {
                    order.push(format!("xp:{}", event.total_experience.0));
                    experience = Some((
                        event.total_experience.0,
                        event.experience_level.0,
                        event.experience_progress.to_bits(),
                    ));
                }
                cb::Packet::SystemChat(message)
                    if message
                        .content
                        .to_json()
                        .is_ok_and(|json| json.contains("commands.summon.success")) =>
                {
                    summoned = true
                }
                _ => {}
            }
            summoned
                && takes == count
                && removed
                && experience.is_some_and(|state| state.0 == expected_xp)
                && (!mend || pickaxe_damage(state) == Some(0))
        },
    )
    .await?;
    ensure!(
        takes == count,
        "{label}: picked up {takes} of {count} merged orbs"
    );
    let remove = order
        .iter()
        .position(|event| event == "remove")
        .context("orb removal packet")?;
    ensure!(
        order[..remove]
            .iter()
            .filter(|event| event.as_str() == "take")
            .count()
            == count as usize,
        "{label}: take/remove order {order:?}"
    );
    ensure!(
        orb.is_some_and(|id| !bot.snapshot().entities.contains_key(&id)),
        "{label}: orb remained visible after its final count"
    );
    log.record(format!("orb.{label}.packet_order"), order)?;
    log.record(
        format!("orb.{label}.experience"),
        experience.context("SetExperience after orb pickup")?,
    )?;
    if mend {
        ensure!(
            pickaxe_damage(&bot.snapshot()) == Some(0),
            "Mending did not repair the selected item"
        );
        log.record("orb.mending.damage", 0)?;
    }
    Ok(())
}

async fn plain_and_mending(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    freeze(bot).await?;
    if bot.snapshot().game_mode != Some(ferraloom_parity::protocol::play::GameType::Survival) {
        command(bot, "gamemode survival", "commands.gamemode.success.self").await?;
    }
    bot.teleport([8.5, -60.0, 8.5]).await?;
    let mut log = ObservationLog::default();
    collect(bot, "plain", 3, 0, 21, false, &mut log).await?;
    command(bot, "give @s minecraft:diamond_pickaxe[minecraft:damage=3,minecraft:enchantments={'minecraft:mending':1}]", "commands.give.success.single").await?;
    if pickaxe_damage(&bot.snapshot()) != Some(3) {
        bot.wait_for(
            "damaged Mending pickaxe inventory",
            Ticks::DEFAULT,
            |state, _| pickaxe_damage(state) == Some(3),
        )
        .await?;
    }
    // Native integer accounting repairs three durability from a seven-XP orb,
    // then returns six XP: 7 - (3 * 7 / 14), followed by a fresh empty selection.
    collect(bot, "mending", 1, 21, 27, true, &mut log).await?;
    session.stop().await?;
    let saved = session.saved_world()?.player("ParityBot")?;
    ensure!(saved.get_int("XpTotal") == Some(27), "saved orb XP total");
    let inventory = saved
        .get_list("Inventory")
        .context("saved player inventory")?;
    let tool = inventory
        .iter()
        .filter_map(|entry| entry.as_compound())
        .find(|entry| entry.get_byte("Slot") == Some(0))
        .context("saved selected Mending item")?;
    let tool = ferraloom_parity::protocol::component::ItemStack::from_nbt(
        &ferraloom_nbt::Tag::Compound(tool.clone()),
    )?;
    let damage = match tool.component(ComponentType::DAMAGE)? {
        Some(DataComponent::Damage(value)) => value.0,
        _ => anyhow::bail!("saved pickaxe damage component"),
    };
    ensure!(damage == 0, "saved Mending damage {damage}");
    log.record("orb.saved_total", saved.get_int("XpTotal"))?;
    log.record("orb.saved_mending_damage", damage)?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both Strict servers and canonical orb, Mending, menu and tracker integration"]
async fn parity_plain_and_mending_orbs_preserve_take_count_repair_xp_and_save() -> Result<()> {
    run(
        level_support::options(),
        RunMode::from_env()?,
        plain_and_mending,
    )
    .await
}
