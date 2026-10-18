use super::{
    AdvancementHolder, AdvancementProgress, AdvancementRewards, Identifier, ObtainedTime, Result,
    ServerAdvancementManager,
    triggers::{Criterion, MatchContext, TriggerEvent, TriggerInstance, TriggerType},
};
use crate::loot::{
    Holder, HolderSet, LootData, LootError, LootParams, LootRandomSources, LootTable, codec,
};
use ferraloom_protocol::{
    chat::{Argument, Component, HoverEvent, Style},
    play::{
        self,
        clientbound::{SelectAdvancementsTab, UpdateAdvancements},
        serverbound::SeenAdvancements,
    },
};
use ferraloom_util::collections::{HashMap as JavaHashMap, HashSet as JavaHashSet, JavaHash};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Identifier value hash used by AdvancementHolder and AdvancementNode in Java.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key(Identifier);
impl JavaHash for Key {
    fn java_hash_code(&self) -> i32 {
        self.0
            .namespace()
            .java_hash_code()
            .wrapping_mul(31)
            .wrapping_add(self.0.path().java_hash_code())
    }
}
/// Port of PlayerAdvancements.TriggerInstanceKey.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerInstanceKey {
    pub advancement: Identifier,
    pub criterion: String,
}
impl JavaHash for TriggerInstanceKey {
    fn java_hash_code(&self) -> i32 {
        Key(self.advancement.clone())
            .java_hash_code()
            .wrapping_mul(31)
            .wrapping_add(self.criterion.java_hash_code())
    }
}
/// Completion effects retain vanilla's call order. The live adapter applies each
/// reward synchronously, including reentrant recipe_unlocked triggers, then chat.
#[derive(Clone, Debug, PartialEq)]
pub enum AdvancementEffect {
    Rewards {
        advancement: Identifier,
        rewards: AdvancementRewards,
    },
    Announcement(Component),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AwardResult {
    pub changed: bool,
    pub effects: Vec<AdvancementEffect>,
}
/// Port of PlayerAdvancements' player-local state. No Level or sockets are owned.
#[derive(Clone, Debug)]
pub struct PlayerAdvancements {
    pub progress: Vec<(Identifier, AdvancementProgress)>,
    visible: JavaHashSet<Key>,
    progress_changed: JavaHashSet<Key>,
    roots_to_update: JavaHashSet<Key>,
    active_triggers: BTreeMap<TriggerType, JavaHashMap<TriggerInstanceKey, TriggerInstance>>,
    pub last_selected_tab: Option<Identifier>,
    first_packet: bool,
}
impl Default for PlayerAdvancements {
    fn default() -> Self {
        Self {
            progress: Vec::new(),
            visible: JavaHashSet::new(),
            progress_changed: JavaHashSet::new(),
            roots_to_update: JavaHashSet::new(),
            active_triggers: BTreeMap::new(),
            last_selected_tab: None,
            first_packet: true,
        }
    }
}
/// Inputs supplied by the live player at the award call site.
#[derive(Clone, Copy)]
pub struct AwardContext<'a> {
    pub now: ObtainedTime,
    pub player_name: &'a Component,
    pub announce_advancements: bool,
}
impl PlayerAdvancements {
    pub fn new(manager: &ServerAdvancementManager) -> Self {
        let mut result = Self {
            first_packet: true,
            ..Self::default()
        };
        result.register_listeners(manager);
        result
    }
    pub fn get_progress(&self, id: &Identifier) -> Option<&AdvancementProgress> {
        self.progress
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, v)| v)
    }
    pub fn get_or_start_progress(
        &mut self,
        holder: &AdvancementHolder,
    ) -> &mut AdvancementProgress {
        let index = self
            .progress
            .iter()
            .position(|(id, _)| id == &holder.id)
            .unwrap_or_else(|| {
                let mut progress = AdvancementProgress::default();
                progress.update(&holder.value.requirements);
                self.progress.push((holder.id.clone(), progress));
                self.progress.len() - 1
            });
        &mut self.progress[index].1
    }
    pub fn clear_triggers(&mut self) {
        self.active_triggers.clear();
    }
    pub fn listeners(
        &self,
        trigger: TriggerType,
    ) -> impl Iterator<Item = (&TriggerInstanceKey, &TriggerInstance)> {
        self.active_triggers
            .get(&trigger)
            .into_iter()
            .flat_map(|map| map.iter())
    }
    pub fn add_player_listener(&mut self, criterion: &Criterion, key: TriggerInstanceKey) {
        self.active_triggers
            .entry(criterion.trigger)
            .or_default()
            .insert(key, criterion.instance.clone());
    }
    pub fn remove_player_listener(&mut self, trigger: TriggerType, key: &TriggerInstanceKey) {
        if let Some(map) = self.active_triggers.get_mut(&trigger) {
            map.remove(key);
            if map.is_empty() {
                self.active_triggers.remove(&trigger);
            }
        }
    }
    pub fn remove_player_listeners(&mut self, trigger: TriggerType) {
        self.active_triggers.remove(&trigger);
    }
    pub fn register_listeners(&mut self, manager: &ServerAdvancementManager) {
        for holder in &manager.advancements {
            self.register(holder);
        }
    }
    fn register(&mut self, holder: &AdvancementHolder) {
        if self.get_or_start_progress(holder).is_done() {
            return;
        }
        for (name, criterion) in &holder.value.criteria {
            if !self.get_or_start_progress(holder).criterion_done(name) {
                self.add_player_listener(
                    criterion,
                    TriggerInstanceKey {
                        advancement: holder.id.clone(),
                        criterion: name.clone(),
                    },
                );
            }
        }
    }
    fn unregister(&mut self, holder: &AdvancementHolder) {
        for (name, criterion) in &holder.value.criteria {
            let progress = self.get_or_start_progress(holder);
            if progress.criterion_done(name) || progress.is_done() {
                self.remove_player_listener(
                    criterion.trigger,
                    &TriggerInstanceKey {
                        advancement: holder.id.clone(),
                        criterion: name.clone(),
                    },
                );
            }
        }
    }
    fn mark_visibility(&mut self, manager: &ServerAdvancementManager, id: &Identifier) {
        if let Some(root) = manager.tree.root(id) {
            self.roots_to_update.insert(Key(root.holder.id.clone()));
        }
    }
    /// The rule argument is GameRules.SHOW_ADVANCEMENT_MESSAGES in 26.3.
    pub fn award(
        &mut self,
        manager: &ServerAdvancementManager,
        id: &Identifier,
        criterion: &str,
        award: AwardContext<'_>,
    ) -> AwardResult {
        let Some(holder) = manager.get(id) else {
            return AwardResult::default();
        };
        let progress = self.get_or_start_progress(holder);
        let was_done = progress.is_done();
        let changed = progress.grant(criterion, award.now);
        let done = progress.is_done();
        let mut effects = Vec::new();
        if changed {
            self.unregister(holder);
            self.progress_changed.insert(Key(id.clone()));
            if !was_done && done {
                effects.push(AdvancementEffect::Rewards {
                    advancement: id.clone(),
                    rewards: holder.value.rewards.clone(),
                });
                if award.announce_advancements
                    && holder
                        .value
                        .display
                        .as_ref()
                        .is_some_and(|d| d.announce_to_chat)
                {
                    effects.push(AdvancementEffect::Announcement(announcement(
                        holder,
                        award.player_name,
                    )));
                }
            }
        }
        if !was_done && done {
            self.mark_visibility(manager, id);
        }
        AwardResult { changed, effects }
    }
    pub fn revoke(
        &mut self,
        manager: &ServerAdvancementManager,
        id: &Identifier,
        criterion: &str,
    ) -> bool {
        let Some(holder) = manager.get(id) else {
            return false;
        };
        let progress = self.get_or_start_progress(holder);
        let was_done = progress.is_done();
        let changed = progress.revoke(criterion);
        let done = progress.is_done();
        if changed {
            self.register(holder);
            self.progress_changed.insert(Key(id.clone()));
        }
        if was_done && !done {
            self.mark_visibility(manager, id);
        }
        changed
    }
    /// Port of SimpleCriterionTrigger#trigger's capture phase. Matching all
    /// listeners precedes any award; callers then apply one award's effects before
    /// the next award so reward callbacks can reenter the engine synchronously.
    pub fn matching_listeners(
        &self,
        trigger: TriggerType,
        player_context: &LootParams,
        context: &mut MatchContext<'_>,
        mut matches: impl FnMut(&TriggerInstance, &mut MatchContext<'_>) -> Result<bool>,
    ) -> Result<Vec<TriggerInstanceKey>> {
        if trigger == TriggerType::Impossible {
            return Ok(Vec::new());
        }
        let mut matched = Vec::new();
        for (key, instance) in self.listeners(trigger) {
            if matches(instance, context)?
                && match &instance.player {
                    None => true,
                    Some(predicate) => context.predicate(predicate, player_context)?,
                }
            {
                matched.push(key.clone());
            }
        }
        Ok(matched)
    }
    /// Pure batch convenience. Live adapters use matching_listeners + award and
    /// apply each result before the next listener to preserve reward reentrancy.
    pub fn trigger(
        &mut self,
        manager: &ServerAdvancementManager,
        trigger: TriggerType,
        event: TriggerEvent<'_>,
        player_context: &LootParams,
        context: &mut MatchContext<'_>,
        award: AwardContext<'_>,
    ) -> Result<Vec<AwardResult>> {
        let matched =
            self.matching_listeners(trigger, player_context, context, |instance, context| {
                instance.matches(event, context)
            })?;
        Ok(matched
            .into_iter()
            .map(|key| self.award(manager, &key.advancement, &key.criterion, award))
            .collect())
    }
    /// Port of PlayerAdvancements#flushDirty. An empty first flush sends nothing
    /// and still consumes first_packet, as vanilla does.
    pub fn flush_dirty(
        &mut self,
        manager: &ServerAdvancementManager,
        show_advancements: bool,
    ) -> Option<UpdateAdvancements> {
        let mut added = JavaHashSet::new();
        let mut removed = JavaHashSet::new();
        for root in self.roots_to_update.iter() {
            for (id, visible) in manager.tree.evaluate_visibility(&root.0, |id| {
                self.get_progress(id)
                    .is_some_and(AdvancementProgress::is_done)
            }) {
                let key = Key(id);
                if visible {
                    if self.visible.insert(key.clone()) {
                        added.insert(key.clone());
                        self.progress_changed.insert(key);
                    }
                } else if self.visible.remove(&key) {
                    removed.insert(key);
                }
            }
        }
        self.roots_to_update.clear();
        let mut progress = JavaHashMap::new();
        for key in self.progress_changed.iter() {
            if self.visible.contains(key) {
                if let Some(value) = self.get_progress(&key.0) {
                    progress.insert(key.clone(), value.network(key.0.clone()));
                }
            }
        }
        self.progress_changed.clear();
        let packet = if added.is_empty() && removed.is_empty() && progress.is_empty() {
            None
        } else {
            Some(UpdateAdvancements {
                should_reset: self.first_packet,
                added: added
                    .iter()
                    .filter_map(|key| manager.tree.get(&key.0))
                    .map(|node| play::PositionedAdvancement {
                        advancement: node.holder.network(),
                        x: node.x,
                        y: node.y,
                    })
                    .collect(),
                removed: removed.iter().map(|key| key.0.clone()).collect(),
                progress: progress.iter().map(|(_, v)| v.clone()).collect(),
                show_advancements,
            })
        };
        self.first_packet = false;
        packet
    }
    pub fn set_selected_tab(
        &mut self,
        holder: Option<&AdvancementHolder>,
    ) -> Option<SelectAdvancementsTab> {
        let selected = holder
            .filter(|h| h.value.parent.is_none() && h.value.display.is_some())
            .map(|h| h.id.clone());
        if selected == self.last_selected_tab {
            return None;
        }
        self.last_selected_tab = selected.clone();
        Some(SelectAdvancementsTab { tab: selected })
    }
    /// Port of ServerGamePacketListenerImpl#handleSeenAdvancements. Unknown tabs
    /// and closing the screen are ignored; known non-root tabs clear selection.
    pub fn handle_seen(
        &mut self,
        manager: &ServerAdvancementManager,
        packet: &SeenAdvancements,
    ) -> Option<SelectAdvancementsTab> {
        match packet {
            SeenAdvancements::ClosedScreen => None,
            SeenAdvancements::OpenedTab(id) => manager
                .get(id)
                .and_then(|holder| self.set_selected_tab(Some(holder))),
        }
    }
    pub fn to_json(&self, offset: impl Fn(ObtainedTime) -> i32) -> Result<Value> {
        let mut result = serde_json::Map::new();
        for (id, progress) in &self.progress {
            if progress.has_progress() {
                result.insert(id.to_string(), progress.to_json(&offset)?);
            }
        }
        result.insert("DataVersion".into(), json!(ferraloom_storage::DATA_VERSION));
        Ok(Value::Object(result))
    }
    /// Port of load/applyFrom. Decode is transactional. Unknown advancements are
    /// reported to the adapter, and loaded progress is ordered by first grant.
    pub fn load_json(
        &mut self,
        manager: &ServerAdvancementManager,
        v: &Value,
    ) -> Result<Vec<Identifier>> {
        let object = codec::object(v)?;
        let version = object
            .get("DataVersion")
            .map(codec::int)
            .transpose()?
            .unwrap_or(1343);
        if version < ferraloom_storage::DATA_VERSION {
            return Err(LootError::Unsupported(format!(
                "advancements.datafix.{version}"
            )));
        }
        let mut loaded = Vec::new();
        for (name, value) in object {
            if name != "DataVersion" {
                loaded.push((
                    Identifier::parse(name)?,
                    AdvancementProgress::decode(value)?,
                ));
            }
        }
        loaded.sort_by_key(|(_, p)| {
            p.first_progress_date()
                .map_or((true, None), |time| (false, Some(time)))
        });
        *self = Self {
            first_packet: true,
            ..Self::default()
        };
        let mut unknown = Vec::new();
        for (id, mut progress) in loaded {
            if let Some(holder) = manager.get(&id) {
                progress.update(&holder.value.requirements);
                self.progress.push((id.clone(), progress));
                self.progress_changed.insert(Key(id.clone()));
                self.mark_visibility(manager, &id);
            } else {
                unknown.push(id);
            }
        }
        self.register_listeners(manager);
        Ok(unknown)
    }
    pub fn save(&self, path: &Path, offset: impl Fn(ObtainedTime) -> i32) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| codec::error(e.to_string()))?;
        }
        let json = serde_json::to_vec_pretty(&self.to_json(offset)?)
            .map_err(|e| codec::error(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| codec::error(e.to_string()))
    }
    pub fn load_file(
        &mut self,
        manager: &ServerAdvancementManager,
        path: &Path,
    ) -> Result<Vec<Identifier>> {
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let file = std::fs::read(path).map_err(|e| codec::error(e.to_string()))?;
        let json = serde_json::from_slice(&file).map_err(|e| codec::error(e.to_string()))?;
        self.load_json(manager, &json)
    }
    pub fn save_path(world: &Path, uuid: ferraloom_protocol::Uuid) -> PathBuf {
        world.join("advancements").join(format!("{uuid}.json"))
    }
}
/// Port of Advancement#decorateName and AdvancementType#createAnnouncement.
pub fn announcement(holder: &AdvancementHolder, player_name: &Component) -> Component {
    let Some(display) = &holder.value.display else {
        return Component::literal(holder.id.to_string());
    };
    let frame = match display.frame {
        play::AdvancementType::Challenge => "challenge",
        play::AdvancementType::Goal => "goal",
        _ => "task",
    };
    let color = Style::from_json(if frame == "challenge" {
        r#"{"color":"dark_purple"}"#
    } else {
        r#"{"color":"green"}"#
    })
    .expect("vanilla color");
    let mut tooltip = display.title.clone();
    tooltip.style = tooltip.style.apply_to(&color);
    tooltip.extra.push(Component::literal("\n"));
    tooltip.extra.push(display.description.clone());
    let mut title = display.title.clone();
    title.style.hover_event = Some(HoverEvent::ShowText(Box::new(tooltip)));
    let mut name = Component::literal("[");
    name.extra.push(title);
    name.extra.push(Component::literal("]"));
    name.style = color;
    Component::translatable(
        format!("chat.type.advancement.{frame}"),
        vec![
            Argument::Component(Box::new(player_name.clone())),
            Argument::Component(Box::new(name)),
        ],
    )
}
impl AdvancementRewards {
    /// Resolve holder tags once before granting, preserving registry encounter order.
    pub fn loot_tables(&self, data: &LootData) -> Result<Vec<Holder<LootTable>>> {
        match &self.loot {
            HolderSet::Direct(tables) => Ok(tables.clone()),
            HolderSet::Tag(tag) => data
                .tags
                .get(&("loot_table", tag.clone()))
                .map(|values| values.iter().cloned().map(Holder::Reference).collect())
                .ok_or_else(|| LootError::MissingReference {
                    registry: "loot_table tag",
                    id: tag.clone(),
                }),
        }
    }
    /// One iteration of AdvancementRewards#grant. Inventory insertion, pickup
    /// sounds, targeted drops and menu notification belong to the live adapter.
    pub fn generate_loot(
        &self,
        table: &Holder<LootTable>,
        data: &LootData,
        params: &LootParams,
        sources: &mut LootRandomSources<'_>,
        facts: &dyn crate::loot::LootFacts,
    ) -> Result<Vec<ferraloom_protocol::component::ItemStack>> {
        data.get_random_items_from_holder(table, params, sources, facts)
    }
    /// Functions remain a policy-routed unsupported effect until functions exist.
    pub fn function_effect(&self) -> Result<()> {
        if let Some(id) = &self.function {
            Err(LootError::Unsupported(format!(
                "advancement.reward.function.{id}"
            )))
        } else {
            Ok(())
        }
    }
}
