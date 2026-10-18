//! Native CriteriaTriggers entry points. Listener matches and award effects run
//! synchronously; missing runtime facts remain policy-routed Unsupported.
use super::{
    live,
    triggers::{TriggerEvent, TriggerType, create_context},
};
use crate::{
    block::UnportedOperation,
    crafting::RecipeHolder,
    entity::{
        EntityData, EntityRef,
        player::{ServerPlayer, inventory},
    },
    inventory::StackRef,
    level::{Level, LevelError},
    loot::LootParams,
};
use ferraloom_protocol::{Identifier, component::ItemStack};
use ferraloom_util::{BlockPos, Vec3};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Trigger {
    GenerateLoot,
    ItemUsedOnBlock,
    DefaultBlockUse,
    AnyBlockUse,
    InventoryChanged,
    RecipeUnlocked,
    Tick,
    Location,
    RewardFunction,
    Native(TriggerType),
}
impl Trigger {
    pub const fn name(self) -> &'static str {
        match self {
            Self::GenerateLoot => "generate_loot",
            Self::ItemUsedOnBlock => "item_used_on_block",
            Self::DefaultBlockUse => "default_block_use",
            Self::AnyBlockUse => "any_block_use",
            Self::InventoryChanged => "inventory_changed",
            Self::RecipeUnlocked => "recipe_unlocked",
            Self::Tick => "tick",
            Self::Location => "location",
            Self::RewardFunction => "advancement_reward_function",
            Self::Native(trigger) => trigger.name(),
        }
    }
    pub(super) const fn from_type(trigger: TriggerType) -> Self {
        match trigger {
            TriggerType::PlayerGeneratesContainerLoot => Self::GenerateLoot,
            TriggerType::ItemUsedOnBlock => Self::ItemUsedOnBlock,
            TriggerType::DefaultBlockUse => Self::DefaultBlockUse,
            TriggerType::AnyBlockUse => Self::AnyBlockUse,
            TriggerType::InventoryChanged => Self::InventoryChanged,
            TriggerType::RecipeUnlocked => Self::RecipeUnlocked,
            TriggerType::Tick => Self::Tick,
            TriggerType::Location => Self::Location,
            trigger => Self::Native(trigger),
        }
    }
}
/// Port of CriteriaTriggers.ITEM_USED_ON_BLOCK.trigger, with the native pre-use
/// item copy supplied by ServerPlayerGameMode#useItemOn.
pub fn item_used_on_block(
    level: &mut Level,
    player: EntityRef,
    pos: BlockPos,
    stack: &ItemStack,
) -> Result<(), LevelError> {
    location_event(
        level,
        player,
        pos,
        Some(stack),
        TriggerType::ItemUsedOnBlock,
    )
}
/// Port of BlockItem#place's PLACED_BLOCK call after setPlacedBy. The caller
/// supplies the original operation-rooted receiver, before sound and consume.
pub fn placed_block(
    level: &mut Level,
    player: EntityRef,
    pos: BlockPos,
    stack: StackRef,
) -> Result<(), LevelError> {
    level.retain_stack(stack).map_err(live::menu_error)?;
    let result = (|| {
        // ItemUsedOnLocationTrigger's predicates read TOOL synchronously and
        // cannot mutate it or observe identity. Resolving its value here is
        // equivalent to vanilla's reference; awards run after every match.
        // ITEM_USED_ON_BLOCK separately receives vanilla's pre-use copy.
        let tool = level.stack(stack).map_err(live::menu_error)?.clone();
        location_event(level, player, pos, Some(&tool), TriggerType::PlacedBlock)
    })();
    level.release_stack(stack);
    result
}

/// Port of ItemDurabilityTrigger#trigger, before the caller changes damage.
pub fn item_durability_changed(
    level: &mut Level,
    player: EntityRef,
    stack: StackRef,
    new_damage: i32,
) -> Result<(), LevelError> {
    level.retain_stack(stack).map_err(live::menu_error)?;
    let result = live::trigger(
        level,
        player,
        TriggerType::ItemDurabilityChanged,
        TriggerEvent::DurabilityLive { stack, new_damage },
        None,
    );
    level.release_stack(stack);
    result
}

/// Port of ConsumeItemTrigger#trigger with the caller's retained receiver.
pub fn consume_item(
    level: &mut Level,
    player: EntityRef,
    stack: StackRef,
) -> Result<(), LevelError> {
    level.retain_stack(stack).map_err(live::menu_error)?;
    let result = live::trigger(
        level,
        player,
        TriggerType::ConsumeItem,
        TriggerEvent::ItemLive(stack),
        None,
    );
    level.release_stack(stack);
    result
}

/// Port of CriteriaTriggers.DEFAULT_BLOCK_USE.trigger (BLOCK_USE parameters).
pub fn default_block_use(
    level: &mut Level,
    player: EntityRef,
    pos: BlockPos,
) -> Result<(), LevelError> {
    location_event(level, player, pos, None, TriggerType::DefaultBlockUse)
}
/// Port of CriteriaTriggers.ANY_BLOCK_USE.trigger, after consumed interaction.
pub fn any_block_use(
    level: &mut Level,
    player: EntityRef,
    pos: BlockPos,
    stack: &ItemStack,
) -> Result<(), LevelError> {
    location_event(level, player, pos, Some(stack), TriggerType::AnyBlockUse)
}
fn location_event(
    level: &mut Level,
    player: EntityRef,
    pos: BlockPos,
    stack: Option<&ItemStack>,
    trigger: TriggerType,
) -> Result<(), LevelError> {
    let mut params = LootParams {
        origin: Some(Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        )),
        this_entity: Some(player),
        block_state: Some(level.get_block_state(pos)?),
        tool: stack.cloned(),
        ..Default::default()
    };
    // DEFAULT_BLOCK_USE deliberately has neither TOOL nor BLOCK_ENTITY.
    let block_entity = if stack.is_some() {
        level.block_entity_ref(pos)?
    } else {
        None
    };
    if let Some(handle) = block_entity {
        let receiver = level.block_entity_state(handle)?;
        params.block_entity = Some(ferraloom_nbt::Tag::Compound(
            receiver
                .save_with_full_metadata()
                .map_err(|_| LevelError::Inventory("advancement block entity NBT"))?,
        ));
        params.block_entity_components = Some(receiver.collect_components());
    }
    live::trigger(
        level,
        player,
        trigger,
        TriggerEvent::Location(&params),
        block_entity,
    )
}
/// Port of CriteriaTriggers.GENERATE_LOOT.trigger before table clearing.
pub fn generate_loot(
    level: &mut Level,
    player: EntityRef,
    table: &Identifier,
) -> Result<(), LevelError> {
    live::trigger(
        level,
        player,
        TriggerType::PlayerGeneratesContainerLoot,
        TriggerEvent::Loot(table),
        None,
    )
}
/// Port of InventoryChangeTrigger#trigger. The argument is the retained native
/// lastSlots copy supplied by triggerSlotListeners, independent of slot kind.
pub fn inventory_changed(
    level: &mut Level,
    player: EntityRef,
    stack: StackRef,
) -> Result<(), LevelError> {
    level.retain_stack(stack).map_err(live::menu_error)?;
    let result = (|| {
        let owner = level.entity::<ServerPlayer>(player)?;
        let (mut full, mut empty, mut occupied) = (0, 0, 0);
        for slot in 0..43 {
            let item = owner.inventory.get_item(slot, &owner.parent.equipment);
            if item.is_empty() {
                empty += 1;
            } else {
                occupied += 1;
                if inventory::count(item) >= inventory::max_stack_size(item) {
                    full += 1;
                }
            }
        }
        live::trigger(
            level,
            player,
            TriggerType::InventoryChanged,
            TriggerEvent::InventoryLive {
                player,
                changed: stack,
                full,
                empty,
                occupied,
            },
            None,
        )
    })();
    level.release_stack(stack);
    result
}
/// Port of RecipeUnlockedTrigger#trigger. The recipe holder belongs to the
/// immutable datapack service, so nested awards never invalidate its borrow.
pub fn recipe_unlocked(
    level: &mut Level,
    player: EntityRef,
    recipe: &RecipeHolder,
) -> Result<(), LevelError> {
    let id = Identifier::parse(&recipe.id.to_string())
        .map_err(|_| LevelError::Inventory("advancement recipe identifier"))?;
    live::trigger(
        level,
        player,
        TriggerType::RecipeUnlocked,
        TriggerEvent::Recipe {
            id: &id,
            ingredients: &[],
        },
        None,
    )
}
/// Port of CriteriaTriggers.RECIPE_CRAFTED at ServerPlayer#triggerRecipeCrafted.
pub fn recipe_crafted(
    level: &mut Level,
    player: EntityRef,
    recipe: &RecipeHolder,
    ingredients: &[StackRef],
) -> Result<(), LevelError> {
    recipe_crafted_event(
        level,
        player,
        recipe,
        ingredients,
        TriggerType::RecipeCrafted,
    )
}
/// Port of CriteriaTriggers.CRAFTER_RECIPE_CRAFTED at CrafterBlock#dispenseItem.
/// The producer captures the eligible nearby players at the native call point.
pub fn crafter_recipe_crafted(
    level: &mut Level,
    player: EntityRef,
    recipe: &RecipeHolder,
    ingredients: &[StackRef],
) -> Result<(), LevelError> {
    recipe_crafted_event(
        level,
        player,
        recipe,
        ingredients,
        TriggerType::CrafterRecipeCrafted,
    )
}
/// Keep ordered native receiver identities, including empty stacks and repeated
/// refs, retained across all synchronous award effects without copying inputs.
fn recipe_crafted_event(
    level: &mut Level,
    player: EntityRef,
    recipe: &RecipeHolder,
    ingredients: &[StackRef],
    trigger: TriggerType,
) -> Result<(), LevelError> {
    for (index, &stack) in ingredients.iter().enumerate() {
        if let Err(error) = level.retain_stack(stack) {
            for &retained in &ingredients[..index] {
                level.release_stack(retained);
            }
            return Err(live::menu_error(error));
        }
    }
    let result = (|| {
        let id = Identifier::parse(&recipe.id.to_string())
            .map_err(|_| LevelError::Inventory("advancement recipe identifier"))?;
        live::trigger(
            level,
            player,
            trigger,
            TriggerEvent::RecipeLive {
                id: &id,
                ingredients,
            },
            None,
        )
    })();
    for &stack in ingredients {
        level.release_stack(stack);
    }
    result
}
/// Port of CriteriaTriggers.TICK at the native ServerPlayer#tick phase.
pub fn tick(level: &mut Level, player: EntityRef) -> Result<(), LevelError> {
    live::trigger(level, player, TriggerType::Tick, TriggerEvent::Player, None)
}
/// Port of VibrationSystem.User#isValidVibration's AVOID_VIBRATION call.
/// The producer owns the careful-stepping and vibration-tag eligibility gates.
pub fn avoid_vibration(level: &mut Level, player: EntityRef) -> Result<(), LevelError> {
    live::trigger(
        level,
        player,
        TriggerType::AvoidVibration,
        TriggerEvent::Player,
        None,
    )
}
/// Port of SculkCatalystBlockEntity.CatalystListener#tryAwardItSpreadsAdvancement.
/// The producer supplies the actual last source or native playerAttack fallback.
pub fn kill_mob_near_sculk_catalyst(
    level: &mut Level,
    player: EntityRef,
    victim: EntityRef,
    source: &crate::entity::damage::DamageSource,
) -> Result<(), LevelError> {
    let origin = level.entity::<EntityData>(player)?.position;
    let entity = create_context(origin, victim);
    let source = source.to_loot_source();
    live::trigger(
        level,
        player,
        TriggerType::KillMobNearSculkCatalyst,
        TriggerEvent::Killed {
            entity: &entity,
            source: &source,
        },
        None,
    )
}
/// Port of CriteriaTriggers.LOCATION at ServerPlayer#doTick's 20-tick tail.
pub fn location(level: &mut Level, player: EntityRef) -> Result<(), LevelError> {
    live::trigger(
        level,
        player,
        TriggerType::Location,
        TriggerEvent::Player,
        None,
    )
}
/// Explicit boundary for trigger families without a producing live subsystem.
pub fn unsupported(level: &mut Level, trigger: TriggerType) -> Result<(), LevelError> {
    level.unported_operation(UnportedOperation::AdvancementCriterion(Trigger::from_type(
        trigger,
    )))
}
