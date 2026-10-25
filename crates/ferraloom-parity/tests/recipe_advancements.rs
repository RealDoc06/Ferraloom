//! Live InventoryChangeTrigger -> AdvancementRewards -> ServerRecipeBook, with saved completion.
use anyhow::{Context, bail};
use ferraloom_data::{
    Item,
    datapack::{self, Pack, ResourceManager, Tags},
};
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        Wire, Writer,
        codec::RegistryContext,
        component::{DataComponentPatch, ItemStack},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, time::Instant};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Trigger {
    Item(String),
    OccupiedSlots,
}
#[derive(Debug)]
struct Target {
    advancement: String,
    criterion: String,
}
type Groups = BTreeMap<Trigger, Vec<Target>>;

fn item_input(predicate: &Value, tags: &Tags<Item>) -> Result<String> {
    let predicate = predicate.as_object().context("inventory item predicate")?;
    ensure!(predicate.len() == 1, "unhandled inventory item predicate");
    let values = predicate.get("items").context("item holder set")?;
    let names: Vec<_> = if let Some(name) = values.as_str() {
        vec![name]
    } else {
        values
            .as_array()
            .context("item holder list")?
            .iter()
            .map(|value| value.as_str().context("item holder id"))
            .collect::<Result<_>>()?
    };
    let mut items = Vec::new();
    for name in names {
        if let Some(tag) = name.strip_prefix('#') {
            let id = tag.parse()?;
            items.extend_from_slice(tags.values.get(&id).context("required item tag")?);
        } else {
            items.push(Item::from_name(name).context("required recipe trigger item")?);
        }
    }
    items
        .into_iter()
        .map(Item::name)
        .min()
        .map(str::to_owned)
        .context("empty recipe trigger holder set")
}

/// Inputs come from the real vanilla resources. One fresh player can prove every target that
/// shares its selected inventory input, without resetting or pre-awarding any advancement.
fn groups() -> Result<Groups> {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").context("builder vanilla jar")?;
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar))?]);
    let tags = datapack::load_tags(&resources, "item", |id, _| Item::from_name(&id.to_string()));
    ensure!(tags.diagnostics.is_empty(), "vanilla item tag diagnostics");
    let mut groups = Groups::new();
    let mut pending = BTreeMap::<String, usize>::new();
    for (id, resource) in resources.registry("advancement") {
        if id.namespace() != "minecraft" || !id.path().starts_with("recipes/") {
            continue;
        }
        let definition = resource.json()?;
        ensure!(
            definition.get("display").is_none(),
            "recipe acquired a display: {id}"
        );
        let criteria = definition["criteria"]
            .as_object()
            .context("advancement criteria")?;
        let mut inventory: Vec<_> = criteria
            .iter()
            .filter(|(_, criterion)| criterion["trigger"] == "minecraft:inventory_changed")
            .collect();
        inventory.sort_by_key(|(name, _)| *name);
        let Some((name, criterion)) = inventory.first() else {
            for criterion in criteria.values() {
                if criterion["trigger"] != "minecraft:recipe_unlocked" {
                    *pending
                        .entry(criterion["trigger"].as_str().context("trigger id")?.into())
                        .or_default() += 1;
                }
            }
            continue;
        };
        let requirements = definition["requirements"]
            .as_array()
            .context("advancement requirements")?;
        ensure!(
            requirements.len() == 1
                && requirements[0].as_array().is_some_and(|group| group
                    .iter()
                    .any(|entry| entry.as_str() == Some(name.as_str()))),
            "recipe requires more than the selected inventory criterion: {id}"
        );
        let conditions = criterion["conditions"]
            .as_object()
            .context("inventory conditions")?;
        ensure!(
            conditions.len() == 1,
            "unhandled inventory conditions: {id}"
        );
        let trigger = if let Some(items) = conditions.get("items") {
            let items = items.as_array().context("inventory predicates")?;
            ensure!(items.len() == 1, "multiple inventory predicates: {id}");
            Trigger::Item(item_input(&items[0], &tags)?)
        } else {
            ensure!(
                conditions.get("slots") == Some(&json!({"occupied":{"min":10}})),
                "unhandled inventory slot condition: {id}"
            );
            Trigger::OccupiedSlots
        };
        groups.entry(trigger).or_default().push(Target {
            advancement: id.to_string(),
            criterion: (*name).clone(),
        });
    }
    ensure!(
        groups.values().map(Vec::len).sum::<usize>() == 1727,
        "inventory recipe advancement inventory changed"
    );
    ensure!(groups.len() == 498, "recipe input groups changed");
    ensure!(
        pending
            == BTreeMap::from([
                ("minecraft:enter_block".into(), 11),
                ("minecraft:impossible".into(), 1),
                ("minecraft:tick".into(), 1)
            ]),
        "pending recipe triggers changed: {pending:?}"
    );
    Ok(groups)
}

fn options(groups: usize) -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        properties: [("gamemode".into(), "creative".into())].into(),
        operators: (0..groups).map(player_name).collect(),
        game_rules: [
            ("minecraft:random_tick_speed".into(), "0".into()),
            ("minecraft:spawn_mobs".into(), "false".into()),
        ]
        .into(),
        ferraloom_binary: std::env::var_os("FERRALOOM_PARITY_BINARY").map(Into::into),
        ..Default::default()
    }
}
fn player_name(index: usize) -> String {
    format!("Recipe{index:04}")
}

#[derive(Default)]
struct Rewards {
    packets: Vec<Value>,
    recipe_progress: usize,
    encoding_error: Option<String>,
    added: usize,
}
impl Rewards {
    fn observe(&mut self, packet: &cb::Packet, context: &dyn RegistryContext) {
        let observed = match packet {
            cb::Packet::RecipeBookAdd(packet) => {
                self.added += packet.entries.len();
                Some(("add", packet_bytes(packet, context)))
            }
            cb::Packet::RecipeBookRemove(packet) => Some(("remove", packet_bytes(packet, context))),
            cb::Packet::RecipeBookSettings(packet) => {
                Some(("settings", packet_bytes(packet, context)))
            }
            cb::Packet::UpdateAdvancements(packet) => {
                self.recipe_progress += packet
                    .progress
                    .iter()
                    .filter(|progress| progress.id.as_str().starts_with("minecraft:recipes/"))
                    .count();
                let mut packet = packet.clone();
                // Completed nodes are visible even without a display. Only obtained wall-clock
                // timestamps vary; keep every definition, removal, criterion and sequence intact.
                for progress in &mut packet.progress {
                    for criterion in &mut progress.criteria {
                        if let Some(obtained) = &mut criterion.obtained {
                            *obtained = 0;
                        }
                    }
                }
                Some(("advancements", packet_bytes(&packet, context)))
            }
            _ => None,
        };
        if let Some((kind, bytes)) = observed {
            match bytes {
                Ok(bytes) => self.packets.push(json!({"kind":kind,"bytes":bytes})),
                Err(error) => self.encoding_error = Some(error.to_string()),
            }
        }
    }
}

/// Re-encode received values using that connection's synchronized IDs and bound tags.
fn packet_bytes(
    packet: &impl Wire,
    context: &dyn RegistryContext,
) -> ferraloom_parity::protocol::Result<Vec<u8>> {
    let mut output = Writer::with_context(context);
    packet.encode(&mut output)?;
    Ok(output.into_inner())
}

async fn input(bot: &mut Bot, trigger: &Trigger) -> Result<Rewards> {
    let context = bot.registry_context()?;
    ensure!(
        bot.snapshot().game_mode == Some(play::GameType::Creative),
        "fresh recipe player must be creative"
    );
    let slots = match trigger {
        Trigger::Item(name) => vec![(36, Item::from_name(name).context("trigger item")?)],
        Trigger::OccupiedSlots => (9..19).map(|slot| (slot, Item::BEDROCK)).collect(),
    };
    let mut rewards = Rewards::default();
    for (slot_num, item) in slots {
        bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
            slot_num,
            item_stack: play::UntrustedItemStack(ItemStack::new(
                item,
                1,
                DataComponentPatch::default(),
            )),
        }))
        .await?;
    }
    // This only fences packet execution. Completion is proved by saved progress, never commands.
    bot.command("scoreboard players get #recipe_capture recipe_capture")
        .await?;
    bot.wait_for("inventory input fence", Ticks::DEFAULT, |_, packet| {
        rewards.observe(packet, context.as_ref());
        matches!(packet, cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains("commands.scoreboard.players.get.success") && text.contains("#recipe_capture")))
    }).await?;
    let time = bot
        .snapshot()
        .game_time
        .context("joined player game time")?;
    // Time is sent before entity ticks. Passing the following update also observes that tick's
    // later PlayerAdvancements#flushDirty packets, even when the input arrived at the boundary.
    bot.wait_for(
        "recipe reward and advancement flush",
        Ticks::DEFAULT,
        |_, packet| {
            rewards.observe(packet, context.as_ref());
            matches!(packet, cb::Packet::SetTime(packet) if packet.game_time > time + 20)
        },
    )
    .await?;
    ensure!(
        rewards.recipe_progress > 0,
        "completed recipes produced no advancement progress: {trigger:?}"
    );
    ensure!(
        rewards.added > 0,
        "inventory input produced no recipe-book reward: {trigger:?}"
    );
    if let Some(error) = &rewards.encoding_error {
        bail!("recipe reward encoding: {error}");
    }
    Ok(rewards)
}

/// Only obtained timestamps vary with wall-clock time; all criteria, done flags and versions stay.
fn normalize_obtained(saved: &mut Value) -> Result<()> {
    for (id, advancement) in saved.as_object_mut().context("saved advancement map")? {
        if id == "DataVersion" {
            continue;
        }
        let criteria = advancement["criteria"]
            .as_object_mut()
            .context("saved criteria map")?;
        for obtained in criteria.values_mut() {
            let timestamp = obtained.as_str().context("obtained timestamp")?;
            ensure!(
                timestamp.len() == 25
                    && timestamp.as_bytes()[4] == b'-'
                    && timestamp.as_bytes()[19] == b' ',
                "unexpected obtained timestamp {timestamp}"
            );
            *obtained = json!("<obtained>");
        }
    }
    Ok(())
}

async fn sweep(session: &mut Session, groups: &Groups) -> Result<ObservationLog> {
    let start = Instant::now();
    let mut log = ObservationLog::default();
    for (index, (trigger, targets)) in groups.iter().enumerate() {
        let id = session.join(&player_name(index)).await?;
        let bot = session.bot(id)?;
        if index == 0 {
            for (command, feedback) in [
                (
                    "scoreboard objectives add recipe_capture dummy",
                    "commands.scoreboard.objectives.add.success",
                ),
                (
                    "scoreboard players set #recipe_capture recipe_capture 1",
                    "commands.scoreboard.players.set.success.single",
                ),
            ] {
                bot.command(command).await?;
                bot.wait_for(command, Ticks::DEFAULT, |_, packet| matches!(packet, cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains(feedback)))).await?;
            }
        }
        let rewards = input(bot, trigger).await?;
        log.record(format!("rewards.{index}"), json!({"targets":targets.iter().map(|target| &target.advancement).collect::<Vec<_>>(),"packets":rewards.packets,"recipe_progress":rewards.recipe_progress}))?;
        bot.disconnect().await?;
        if (index + 1) % 50 == 0 {
            eprintln!(
                "recipe inventory groups {}/{} elapsed {:?}",
                index + 1,
                groups.len(),
                start.elapsed()
            );
        }
    }
    session.stop().await?;
    let world = session.saved_world()?;
    for (index, targets) in groups.values().enumerate() {
        let mut saved = world.player_advancements(&player_name(index))?;
        for target in targets {
            let progress = &saved[&target.advancement];
            ensure!(
                progress["done"] == true && progress["criteria"][&target.criterion].is_string(),
                "{} did not complete from {}: {progress}",
                target.advancement,
                target.criterion
            );
        }
        normalize_obtained(&mut saved)?;
        log.record(format!("saved.{index}"), saved)?;
    }
    eprintln!(
        "recipe inventory sweep: 1727 advancements, {} fresh players, {:?}",
        groups.len(),
        start.elapsed()
    );
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires real vanilla resources and disposable Strict vanilla/Ferraloom servers"]
async fn parity_recipe_inventory_advancements() -> Result<()> {
    let groups = groups()?;
    ferraloom_parity::run(
        options(groups.len()),
        RunMode::from_env()?,
        async |session| sweep(session, &groups).await,
    )
    .await
}
