//! Live builtin non-recipe criteria. Inputs and completion requirements come from vanilla.
mod physical;

use super::{Rewards, item_input, normalize_obtained};
use anyhow::Context;
use ferraloom_data::{
    Item,
    datapack::{self, Pack, ResourceManager},
};
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{
        Identifier,
        codec::RegistryContext,
        component::{DataComponentPatch, ItemStack},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use serde_json::{Value, json};
use std::path::Path;

struct Case {
    id: String,
    requirements: Vec<Vec<String>>,
    items: Vec<Item>,
    experience: i32,
}
fn resources() -> Result<ResourceManager> {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").context("builder vanilla jar")?;
    Ok(ResourceManager::new(vec![Pack::vanilla_jar(Path::new(
        &jar,
    ))?]))
}
fn case(id: String, definition: &Value, items: Vec<Item>) -> Result<Case> {
    Ok(Case {
        id,
        requirements: serde_json::from_value(definition["requirements"].clone())?,
        items,
        experience: definition["rewards"]["experience"]
            .as_i64()
            .unwrap_or(0)
            .try_into()?,
    })
}
fn inventory_cases() -> Result<Vec<Case>> {
    let resources = resources()?;
    let tags = datapack::load_tags(&resources, "item", |id, _| Item::from_name(&id.to_string()));
    ensure!(tags.diagnostics.is_empty(), "vanilla item tag diagnostics");
    let mut cases = Vec::new();
    for (id, resource) in resources.registry("advancement") {
        if id.namespace() != "minecraft" || id.path().starts_with("recipes/") {
            continue;
        }
        let definition = resource.json()?;
        let requirements: Vec<Vec<String>> =
            serde_json::from_value(definition["requirements"].clone())?;
        let criteria = definition["criteria"]
            .as_object()
            .context("advancement criteria")?;
        // Every AND group must have a real inventory criterion. In particular,
        // salvage_sherd's independent loot requirement cannot be claimed here.
        let mut selected = Vec::new();
        for group in &requirements {
            let mut options: Vec<_> = group
                .iter()
                .filter(|name| criteria[*name]["trigger"] == "minecraft:inventory_changed")
                .collect();
            options.sort();
            if let Some(name) = options.first() {
                selected.push(&criteria[*name]);
            }
        }
        if selected.len() != requirements.len() {
            continue;
        }
        let mut items = Vec::new();
        for criterion in selected {
            let conditions = criterion["conditions"]
                .as_object()
                .context("inventory conditions")?;
            ensure!(
                conditions.len() == 1,
                "unhandled non-recipe inventory condition: {id}"
            );
            for predicate in conditions["items"]
                .as_array()
                .context("inventory predicates")?
            {
                let name = item_input(predicate, &tags)?;
                items.push(Item::from_name(&name).context("inventory input item")?);
            }
        }
        items.sort_by_key(|item| item.name());
        items.dedup();
        ensure!(
            !items.is_empty() && items.len() <= 27,
            "invalid inventory inputs: {id}"
        );
        cases.push(case(id.to_string(), &definition, items)?);
    }
    cases.sort_by_key(|case| case.id.clone());
    ensure!(
        cases.len() == 21,
        "non-recipe inventory target set changed: {}",
        cases.len()
    );
    Ok(cases)
}
fn completed(progress: &play::AdvancementProgress, requirements: &[Vec<String>]) -> bool {
    requirements.iter().all(|group| {
        group.iter().any(|name| {
            progress
                .criteria
                .iter()
                .any(|criterion| &criterion.name == name && criterion.obtained.is_some())
        })
    })
}

/// Reuses the recipe sweep's exact packet observation. The two-JVM proof covered
/// all native trees: display y remains identity-derived when the display is the
/// target itself. Every x, recipe position, definition field and packet order stays.
struct Capture<'a> {
    case: &'a Case,
    rewards: Rewards,
    progress: Option<play::AdvancementProgress>,
}
impl<'a> Capture<'a> {
    fn new(case: &'a Case, bot: &Bot) -> Result<Self> {
        let id = Identifier::parse(&case.id)?;
        ensure!(
            !bot.snapshot()
                .advancement_progress
                .get(&id)
                .is_some_and(|progress| completed(progress, &case.requirements)),
            "target already completed before input: {}",
            case.id
        );
        Ok(Self {
            case,
            rewards: Rewards::default(),
            progress: None,
        })
    }
    fn observe(&mut self, packet: &cb::Packet, context: &dyn RegistryContext) {
        self.rewards.observe(packet, context);
        if let cb::Packet::UpdateAdvancements(packet) = packet
            && let Some(progress) = packet
                .progress
                .iter()
                .find(|progress| progress.id.as_str() == self.case.id)
        {
            self.progress = Some(progress.clone());
        }
    }
    fn done(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| completed(progress, &self.case.requirements))
    }
    fn record(&self, log: &mut ObservationLog) -> Result<()> {
        ensure!(
            self.done(),
            "target progress did not complete after real input: {}",
            self.case.id
        );
        ensure!(
            self.rewards.encoding_error.is_none(),
            "reward encoding: {:?}",
            self.rewards.encoding_error
        );
        log.record(format!("packets.{}", self.case.id), &self.rewards.packets)
    }
}
fn name(group: &str, index: usize) -> String {
    format!("{group}{index:04}")
}
fn options(group: &str, count: usize) -> LaunchOptions {
    let mut options = super::options(0);
    options.operators = (0..count).map(|index| name(group, index)).collect();
    options
}
async fn command(bot: &mut Bot, command: &str, feedback: &str) -> Result<()> {
    bot.command(command).await?;
    bot.wait_for(command,Ticks::DEFAULT,|_,packet| matches!(packet,cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains(feedback)))).await?;
    Ok(())
}
async fn initialize(bot: &mut Bot) -> Result<()> {
    command(
        bot,
        "scoreboard objectives add recipe_capture dummy",
        "commands.scoreboard.objectives.add.success",
    )
    .await?;
    command(
        bot,
        "scoreboard players set #recipe_capture recipe_capture 1",
        "commands.scoreboard.players.set.success.single",
    )
    .await
}
async fn set_item(bot: &Bot, slot_num: i16, item: Item, count: i32) -> Result<()> {
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num,
        item_stack: play::UntrustedItemStack(ItemStack::new(
            item,
            count,
            DataComponentPatch::default(),
        )),
    }))
    .await
}
async fn flush(bot: &mut Bot, capture: &mut Capture<'_>, frozen: bool) -> Result<()> {
    let context = bot.registry_context()?;
    bot.command("scoreboard players get #recipe_capture recipe_capture")
        .await?;
    bot.wait_for("input execution fence",Ticks::DEFAULT,|_,packet|{
        capture.observe(packet,context.as_ref());
        matches!(packet,cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains("commands.scoreboard.players.get.success") && text.contains("#recipe_capture")))
    }).await?;
    if frozen {
        // Players continue ticking while the world is frozen. The target's full
        // UpdateAdvancements proves the native post-award flush, without stepping
        // unrelated scheduled block ticks or block entities in the fixture.
        if !capture.done() {
            bot.wait_for(
                "completed target advancement flush",
                Ticks::DEFAULT,
                |_, packet| {
                    capture.observe(packet, context.as_ref());
                    capture.done()
                },
            )
            .await?;
        }
    } else {
        let time = bot.snapshot().game_time.context("joined game time")?;
        bot.wait_for(
            "next advancement flush boundary",
            Ticks::DEFAULT,
            |_, packet| {
                capture.observe(packet, context.as_ref());
                matches!(packet,cb::Packet::SetTime(packet) if packet.game_time > time + 20)
            },
        )
        .await?;
    }
    ensure!(capture.done(), "target incomplete: {}", capture.case.id);
    Ok(())
}
fn saved(session: &Session, group: &str, cases: &[Case], log: &mut ObservationLog) -> Result<()> {
    let world = session.saved_world()?;
    for (index, case) in cases.iter().enumerate() {
        let player = name(group, index);
        let mut progress = world.player_advancements(&player)?;
        let target = &progress[&case.id];
        ensure!(
            target["done"] == true,
            "saved advancement incomplete: {}: {target}",
            case.id
        );
        for requirement in &case.requirements {
            ensure!(
                requirement
                    .iter()
                    .any(|name| target["criteria"][name].is_string()),
                "saved criterion group incomplete: {}",
                case.id
            );
        }
        normalize_obtained(&mut progress)?;
        log.record(format!("saved.{}", case.id), progress)?;
        let player = world.player(&player)?;
        let xp = player.get_int("XpTotal").context("saved XpTotal")?;
        ensure!(
            xp == case.experience,
            "advancement XP reward {}: expected {}, got {xp}",
            case.id,
            case.experience
        );
        let level = player.get_int("XpLevel").context("saved XpLevel")?;
        let progress = player.get_float("XpP").context("saved XpP")?;
        log.record(
            format!("experience.{}", case.id),
            json!({"total":xp,"level":level,"progress":progress.to_bits()}),
        )?;
    }
    Ok(())
}
async fn inventory(session: &mut Session, cases: &[Case]) -> Result<ObservationLog> {
    let mut log = ObservationLog::default();
    for (index, case) in cases.iter().enumerate() {
        let player = session.join(&name("Inv", index)).await?;
        let bot = session.bot(player)?;
        ensure!(
            bot.snapshot().game_mode == Some(play::GameType::Creative),
            "creative inventory setup required"
        );
        if index == 0 {
            initialize(bot).await?;
        }
        let mut capture = Capture::new(case, bot)?;
        for (offset, item) in case.items.iter().enumerate() {
            set_item(bot, 9 + i16::try_from(offset)?, *item, 1).await?;
        }
        flush(bot, &mut capture, false).await?;
        capture.record(&mut log)?;
        bot.disconnect().await?;
    }
    session.stop().await?;
    saved(session, "Inv", cases, &mut log)?;
    eprintln!(
        "non-recipe inventory: {} complete builtin advancements",
        cases.len()
    );
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires disposable Strict vanilla/Ferraloom servers and real vanilla resources"]
async fn parity_non_recipe_inventory_advancements() -> Result<()> {
    let cases = inventory_cases()?;
    ferraloom_parity::run(
        options("Inv", cases.len()),
        RunMode::from_env()?,
        async |session| inventory(session, &cases).await,
    )
    .await
}
