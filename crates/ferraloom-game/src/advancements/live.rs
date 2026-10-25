//! Synchronous live adapter for SimpleCriterionTrigger and PlayerAdvancements.
//! Captured listener membership is immutable; canonical player state is borrowed
//! only between predicates and callbacks, including nested recipe awards.
use super::{
    AdvancementEffect, AdvancementRewards, ObtainedTime, PlayerAdvancements,
    triggers::{
        MatchContext, TriggerEvent, TriggerFacts, TriggerRuntime, TriggerType, create_context,
    },
};
use crate::{
    block::{UnportedOperation, containers::loot_runtime::LiveLootRuntime, entity::BlockEntityRef},
    crafting::RecipeHolder,
    entity::{EntityData, EntityRef, item::ItemEntity, player::ServerPlayer},
    inventory::{MenuError, StackRef},
    level::{Level, LevelError, effects::LevelEffect},
    loot::{Identifier, LootError, LootRandomSelection},
};
use ferraloom_protocol::{
    component::ItemStack,
    play::{clientbound as cb, serverbound::SeenAdvancements},
};
use ferraloom_util::random::RandomSource;

impl TriggerFacts for Level {
    fn recipe_tag(&self, tag: &Identifier) -> crate::loot::Result<Vec<Identifier>> {
        self.container_resources()
            .map_err(fact_error)?
            .recipe_tags
            .get(tag)
            .cloned()
            .ok_or_else(|| LootError::MissingReference {
                registry: "recipe tag",
                id: tag.clone(),
            })
    }
    fn loot_table_tag(&self, tag: &Identifier) -> crate::loot::Result<Vec<Identifier>> {
        self.container_resources()
            .map_err(fact_error)?
            .loot
            .tags
            .get(&("loot_table", tag.clone()))
            .cloned()
            .ok_or_else(|| LootError::MissingReference {
                registry: "loot table tag",
                id: tag.clone(),
            })
    }
}
impl TriggerRuntime for LiveLootRuntime<'_> {
    fn trigger_facts(&self) -> &dyn TriggerFacts {
        self.level()
    }
    fn stack(&self, stack: StackRef) -> crate::loot::Result<&ItemStack> {
        self.level()
            .stack(stack)
            .map_err(|error| LootError::Unsupported(error.to_string()))
    }
    fn inventory_item(&self, player: EntityRef, slot: usize) -> crate::loot::Result<&ItemStack> {
        let player = self
            .level()
            .entity::<ServerPlayer>(player)
            .map_err(fact_error)?;
        Ok(player.inventory.get_item(slot, &player.parent.equipment))
    }
}
fn fact_error(error: LevelError) -> LootError {
    LootError::Unsupported(format!("advancement.live.{error:?}"))
}
pub(super) fn menu_error(error: MenuError) -> LevelError {
    match error {
        MenuError::Level(error) => error,
        MenuError::Unsupported(operation) => LevelError::Inventory(operation),
        _ => LevelError::Inventory("advancement inventory receiver"),
    }
}
/// Port of SimpleCriterionTrigger#trigger followed by Listener#run. Evaluate
/// every listener before awarding the captured matches, preserving Java order.
pub(super) fn trigger(
    level: &mut Level,
    player: EntityRef,
    trigger: TriggerType,
    event: TriggerEvent<'_>,
    block_entity: Option<BlockEntityRef>,
) -> Result<(), LevelError> {
    let listeners: Vec<_> = level
        .entity::<ServerPlayer>(player)?
        .advancements
        .listeners(trigger)
        .map(|(key, instance)| (key.clone(), instance.clone()))
        .collect();
    if listeners.is_empty() {
        return Ok(());
    }
    let resources = level.container_resources()?;
    let origin = level.entity::<EntityData>(player)?.position;
    let player_context = create_context(origin, player);
    let matched = {
        let mut runtime = LiveLootRuntime::new(level, LootRandomSelection::Level);
        if let Some(handle) = block_entity
            && let Err(error) = runtime.bind_block_entity_source(handle)
        {
            return criterion_error(runtime.level_mut(), trigger, error);
        }
        let mut context = MatchContext::live(&resources.loot, &mut runtime, origin);
        PlayerAdvancements::match_captured_listeners(
            listeners.iter().map(|(key, instance)| (key, instance)),
            &player_context,
            &mut context,
            |instance, context| instance.matches(event, context),
        )
    };
    let matched = match matched {
        Ok(matched) => matched,
        Err(error) => return criterion_error(level, trigger, error),
    };
    for key in matched {
        let result = level
            .entity_mut::<ServerPlayer>(player)?
            .advancements
            .award(
                &resources.advancements,
                &key.advancement,
                &key.criterion,
                ObtainedTime::now(),
            );
        for effect in result.effects {
            apply_effect(level, player, &resources.advancements, effect)?;
        }
    }
    Ok(())
}
/// Apply one ordered award effect after the preceding effect has returned.
pub(super) fn apply_effect(
    level: &mut Level,
    player: EntityRef,
    manager: &super::ServerAdvancementManager,
    effect: AdvancementEffect,
) -> Result<(), LevelError> {
    match effect {
        AdvancementEffect::Rewards { rewards, .. } => grant_rewards(level, player, &rewards)?,
        AdvancementEffect::AnnounceAdvancement(id) => {
            if !level.rules().show_advancement_messages {
                return Ok(());
            }
            let name = level.entity_display_name(player)?;
            let holder = manager.get(&id).expect("awarded advancement");
            let content = super::announcement(holder, &name);
            let recipients = level
                .all_entities()
                .into_iter()
                .filter(|id| level.entity::<ServerPlayer>(*id).is_ok())
                .collect();
            level.emit(LevelEffect::PlayerPacket {
                recipients,
                packet: Box::new(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                    content,
                    overlay: false,
                }))),
            });
        }
        AdvancementEffect::MarkVisibility(id) => {
            level
                .entity_mut::<ServerPlayer>(player)?
                .advancements
                .apply_mark_visibility(manager, &id);
        }
    }
    Ok(())
}
pub(super) fn criterion_error(
    level: &mut Level,
    trigger: TriggerType,
    error: LootError,
) -> Result<(), LevelError> {
    match error {
        LootError::Unsupported(source) => level
            .unported_operation(UnportedOperation::AdvancementCriterion(
                super::criteria::Trigger::from_type(trigger),
            ))
            .map_err(|error| match error.root() {
                LevelError::UnsupportedAdvancementCriterion(trigger) => {
                    LevelError::UnsupportedAdvancementPredicate {
                        trigger: *trigger,
                        source,
                    }
                }
                _ => error,
            }),
        _ => Err(LevelError::Inventory("advancement predicate evaluation")),
    }
}
/// Port of AdvancementRewards#grant. Each nested recipe_unlocked award finishes
/// synchronously before processing the next recipe or the outer announcement.
fn grant_rewards(
    level: &mut Level,
    player: EntityRef,
    rewards: &AdvancementRewards,
) -> Result<(), LevelError> {
    crate::entity::player::give_experience_points(player, rewards.experience, level)?;
    let resources = level.container_resources()?;
    let params = create_context(level.entity::<EntityData>(player)?.position, player);
    let tables = match rewards.loot_tables(&resources.loot) {
        Ok(tables) => tables,
        Err(error) => return criterion_error(level, TriggerType::RecipeUnlocked, error),
    };
    let mut changes = false;
    for table in tables {
        let sequence = table
            .resolve(&resources.loot)
            .map_err(|_| LevelError::Inventory("advancement reward table"))?
            .random_sequence
            .clone();
        let items = {
            let mut runtime = LiveLootRuntime::new(
                level,
                sequence.map_or(LootRandomSelection::Level, LootRandomSelection::Sequence),
            );
            resources
                .loot
                .get_random_items_from_holder_with_runtime(&table, &params, &mut runtime)
        };
        let items = match items {
            Ok(items) => items,
            Err(error) => return criterion_error(level, TriggerType::RecipeUnlocked, error),
        };
        for mut item in items {
            if crate::inventory::add_item(player, &mut item, level)? {
                let owner = level.entity_mut::<EntityData>(player)?;
                let pitch =
                    ((owner.random.next_float() - owner.random.next_float()) * 0.7 + 1.0) * 2.0;
                let position = owner.position;
                level.play_sound(
                    None,
                    position,
                    ferraloom_data::registries::sound_event::Id::ENTITY_ITEM_PICKUP,
                    ferraloom_protocol::play::SoundSource::Players,
                    0.2,
                    pitch,
                )?;
                changes = true;
            } else if let Some(drop) = crate::entity::player::drop_item(player, item, false, level)?
            {
                let uuid = level.entity::<ServerPlayer>(player)?.profile_id();
                let drop = level.entity_mut::<ItemEntity>(drop)?;
                drop.pickup_delay = 0;
                drop.target = Some(uuid);
            }
        }
    }
    if changes {
        crate::inventory::broadcast_container_menu(player, level)?;
    }
    award_recipes(level, player, rewards.recipe_holders(&resources.recipes))?;
    if rewards.function.is_some() {
        level.unported_operation(UnportedOperation::AdvancementCriterion(
            super::criteria::Trigger::RewardFunction,
        ))?;
    }
    Ok(())
}
/// Port of ServerPlayer#awardRecipes and ServerRecipeBook#addRecipes. Each
/// successful unlock finishes its synchronous criterion effects before the next
/// holder. The aggregate packet and return value count recipe display entries.
pub fn award_recipes<'a>(
    level: &mut Level,
    player: EntityRef,
    recipes: impl IntoIterator<Item = &'a RecipeHolder>,
) -> Result<usize, LevelError> {
    let resources = level.container_resources()?;
    let mut entries = Vec::new();
    for recipe in recipes {
        let addition = level
            .entity_mut::<ServerPlayer>(player)?
            .recipe_book
            .unlock_recipe(recipe, &resources.recipe_synchronization);
        if let Some(addition) = addition {
            entries.extend(addition);
            super::criteria::recipe_unlocked(level, player, recipe)?;
        }
    }
    let count = entries.len();
    if count != 0 {
        send(
            level,
            player,
            cb::Packet::RecipeBookAdd(cb::RecipeBookAdd {
                entries,
                replace: false,
            }),
        );
    }
    Ok(count)
}
/// Capture outbound packets at the native call site; no connection is borrowed.
fn send(level: &mut Level, player: EntityRef, packet: cb::Packet) {
    level.emit(LevelEffect::PlayerPacket {
        recipients: vec![player],
        packet: Box::new(packet),
    });
}
/// Port of PlayerAdvancements#flushDirty.
pub fn flush_dirty(level: &mut Level, player: EntityRef) -> Result<(), LevelError> {
    let resources = level.container_resources()?;
    if let Some(packet) = level
        .entity_mut::<ServerPlayer>(player)?
        .advancements
        .flush_dirty(&resources.advancements, true)
    {
        send(level, player, cb::Packet::UpdateAdvancements(packet));
    }
    Ok(())
}
/// Port of ServerGamePacketListenerImpl#handleSeenAdvancements.
pub fn seen_advancements(
    level: &mut Level,
    player: EntityRef,
    packet: &SeenAdvancements,
) -> Result<(), LevelError> {
    let resources = level.container_resources()?;
    if let Some(packet) = level
        .entity_mut::<ServerPlayer>(player)?
        .advancements
        .handle_seen(&resources.advancements, packet)
    {
        send(level, player, cb::Packet::SelectAdvancementsTab(packet));
    }
    Ok(())
}
