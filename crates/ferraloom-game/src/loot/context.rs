use super::{Identifier, LootError, Result};
use crate::loot::codec::DecodeInput;
use crate::{entity::EntityRef, predicates::PredicateContext};
use ferraloom_data::BlockState;
use ferraloom_nbt::Tag;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Vec3, random::RandomSource};
use std::collections::BTreeSet;

/// Port of LootContextParams. Values remain typed in `LootParams`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextParam {
    ThisEntity,
    LastDamagePlayer,
    DamageSource,
    AttackingEntity,
    DirectAttackingEntity,
    Origin,
    BlockState,
    BlockEntity,
    Tool,
    ExplosionRadius,
    AdditionalCostComponentAllowed,
    Container,
    InteractingEntity,
    TargetEntity,
    EnchantmentActive,
    EnchantmentLevel,
}
/// Port of LootContext.EntityTarget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityTarget {
    This,
    Attacker,
    DirectAttacker,
    AttackingPlayer,
    TargetEntity,
    InteractingEntity,
}
impl EntityTarget {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        use super::codec::{error, string};
        Ok(match string(v)? {
            "this" => Self::This,
            "attacker" => Self::Attacker,
            "direct_attacker" => Self::DirectAttacker,
            "attacking_player" => Self::AttackingPlayer,
            "target_entity" => Self::TargetEntity,
            "interacting_entity" => Self::InteractingEntity,
            name => return Err(error(format!("unknown entity target {name}"))),
        })
    }
    pub fn param(self) -> ContextParam {
        match self {
            Self::This => ContextParam::ThisEntity,
            Self::Attacker => ContextParam::AttackingEntity,
            Self::DirectAttacker => ContextParam::DirectAttackingEntity,
            Self::AttackingPlayer => ContextParam::LastDamagePlayer,
            Self::TargetEntity => ContextParam::TargetEntity,
            Self::InteractingEntity => ContextParam::InteractingEntity,
        }
    }
}
/// Damage source identity and entities; its remaining properties are live facts.
#[derive(Clone, Debug)]
pub struct DamageSource {
    pub damage_type: Identifier,
    pub source: Option<EntityRef>,
    pub direct: Option<EntityRef>,
    pub position: Option<Vec3>,
}

/// Port of LootParams. `None` means absent, including an absent empty-stack parameter.
#[derive(Clone, Debug, Default)]
pub struct LootParams {
    pub luck: f32,
    pub origin: Option<Vec3>,
    pub this_entity: Option<EntityRef>,
    pub attacking_entity: Option<EntityRef>,
    pub direct_attacking_entity: Option<EntityRef>,
    pub last_damage_player: Option<EntityRef>,
    pub target_entity: Option<EntityRef>,
    pub interacting_entity: Option<EntityRef>,
    pub damage_source: Option<DamageSource>,
    pub block_state: Option<BlockState>,
    pub block_entity: Option<Tag>,
    pub block_entity_components: Option<ferraloom_protocol::component::DataComponentMap>,
    pub tool: Option<ItemStack>,
    pub explosion_radius: Option<f32>,
    pub additional_cost_component_allowed: Option<bool>,
    pub container: Option<Vec<ItemStack>>,
    pub enchantment_active: Option<bool>,
    pub enchantment_level: Option<i32>,
}
impl LootParams {
    pub fn entity(&self, target: EntityTarget) -> Option<EntityRef> {
        match target {
            EntityTarget::This => self.this_entity,
            EntityTarget::Attacker => self.attacking_entity,
            EntityTarget::DirectAttacker => self.direct_attacking_entity,
            EntityTarget::AttackingPlayer => self.last_damage_player,
            EntityTarget::TargetEntity => self.target_entity,
            EntityTarget::InteractingEntity => self.interacting_entity,
        }
    }
    pub fn present(&self) -> BTreeSet<ContextParam> {
        use ContextParam::*;
        let entries = [
            (ThisEntity, self.this_entity.is_some()),
            (LastDamagePlayer, self.last_damage_player.is_some()),
            (DamageSource, self.damage_source.is_some()),
            (AttackingEntity, self.attacking_entity.is_some()),
            (
                DirectAttackingEntity,
                self.direct_attacking_entity.is_some(),
            ),
            (Origin, self.origin.is_some()),
            (BlockState, self.block_state.is_some()),
            (
                BlockEntity,
                self.block_entity.is_some() || self.block_entity_components.is_some(),
            ),
            (Tool, self.tool.is_some()),
            (ExplosionRadius, self.explosion_radius.is_some()),
            (
                AdditionalCostComponentAllowed,
                self.additional_cost_component_allowed.is_some(),
            ),
            (Container, self.container.is_some()),
            (InteractingEntity, self.interacting_entity.is_some()),
            (TargetEntity, self.target_entity.is_some()),
            (EnchantmentActive, self.enchantment_active.is_some()),
            (EnchantmentLevel, self.enchantment_level.is_some()),
        ];
        entries
            .into_iter()
            .filter_map(|(key, present)| present.then_some(key))
            .collect()
    }
    /// Port of LootParams.Builder#create. This checks the caller's context set,
    /// not a referenced table's declared validation set.
    pub fn validate(&self, set: &ContextKeySet) -> Result<()> {
        let present = self.present();
        let missing = set
            .required
            .difference(&present)
            .copied()
            .collect::<Vec<_>>();
        let extra = present
            .difference(&set.allowed)
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() && extra.is_empty() {
            Ok(())
        } else {
            Err(LootError::InvalidParameters { missing, extra })
        }
    }
}
/// Port of ContextKeySet and LootContextParamSets.
#[derive(Clone, Debug)]
pub struct ContextKeySet {
    pub required: BTreeSet<ContextParam>,
    pub allowed: BTreeSet<ContextParam>,
}
impl ContextKeySet {
    pub fn vanilla(id: &Identifier) -> Option<Self> {
        use ContextParam::*;
        let (required, optional): (&[ContextParam], &[ContextParam]) = match id.as_str() {
            "minecraft:empty" => (&[], &[]),
            "minecraft:generic" => (
                &[
                    ThisEntity,
                    LastDamagePlayer,
                    DamageSource,
                    AttackingEntity,
                    DirectAttackingEntity,
                    Origin,
                    BlockState,
                    BlockEntity,
                    Tool,
                    ExplosionRadius,
                    AdditionalCostComponentAllowed,
                    Container,
                    InteractingEntity,
                    TargetEntity,
                    EnchantmentActive,
                    EnchantmentLevel,
                ],
                &[],
            ),
            "minecraft:chest" => (&[Origin], &[ThisEntity]),
            "minecraft:command" => (&[Origin], &[ThisEntity]),
            "minecraft:command_slot_source" => (&[Origin, Container], &[ThisEntity]),
            "minecraft:command_compute_default" => (&[Origin], &[ThisEntity]),
            "minecraft:command_compute_position" => {
                (&[Origin, BlockState], &[ThisEntity, BlockEntity])
            }
            "minecraft:command_compute_entity" => (&[Origin, TargetEntity], &[ThisEntity]),
            "minecraft:selector" => (&[Origin, ThisEntity], &[]),
            "minecraft:villager_trade" => {
                (&[Origin, ThisEntity, AdditionalCostComponentAllowed], &[])
            }
            "minecraft:fishing" => (&[Origin, Tool], &[ThisEntity]),
            "minecraft:entity" => (
                &[ThisEntity, Origin, DamageSource],
                &[AttackingEntity, DirectAttackingEntity, LastDamagePlayer],
            ),
            "minecraft:equipment" => (&[Origin, ThisEntity], &[]),
            "minecraft:archaeology" => (&[Origin, ThisEntity, Tool], &[]),
            "minecraft:gift" => (&[Origin, ThisEntity], &[]),
            "minecraft:barter" => (&[ThisEntity], &[]),
            "minecraft:vault" => (&[Origin], &[ThisEntity, Tool]),
            "minecraft:advancement_reward" => (&[ThisEntity, Origin], &[]),
            "minecraft:advancement_entity" => (&[ThisEntity, Origin], &[]),
            "minecraft:advancement_location" => {
                (&[ThisEntity, Origin, Tool, BlockState], &[BlockEntity])
            }
            "minecraft:block_use" => (&[ThisEntity, Origin, BlockState], &[]),
            "minecraft:block" => (
                &[BlockState, Origin, Tool],
                &[ThisEntity, BlockEntity, ExplosionRadius],
            ),
            "minecraft:shearing" => (&[Origin, ThisEntity, Tool], &[]),
            "minecraft:entity_interact" => (&[TargetEntity, Tool], &[InteractingEntity]),
            "minecraft:block_interact" => (
                &[BlockState, Origin],
                &[BlockEntity, InteractingEntity, Tool],
            ),
            "minecraft:container_process" => (&[BlockEntity, BlockState, Container, Origin], &[]),
            "minecraft:enchanted_damage" => (
                &[ThisEntity, EnchantmentLevel, Origin, DamageSource],
                &[DirectAttackingEntity, AttackingEntity],
            ),
            "minecraft:enchanted_item" => (&[Tool, EnchantmentLevel], &[]),
            "minecraft:enchanted_location" => (
                &[ThisEntity, EnchantmentLevel, Origin, EnchantmentActive],
                &[],
            ),
            "minecraft:enchanted_entity" => (&[ThisEntity, EnchantmentLevel, Origin], &[]),
            "minecraft:hit_block" => (&[ThisEntity, EnchantmentLevel, Origin, BlockState], &[]),
            _ => return None,
        };
        Some(Self {
            required: required.iter().copied().collect(),
            allowed: required.iter().chain(optional).copied().collect(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScoreHolder<'a> {
    Entity(EntityRef),
    Name(&'a str),
}
/// Cold gameplay facts used by loot. Implementations read canonical world state.
/// The default adapter deliberately makes unavailable facts explicit.
pub trait LootFacts: PredicateContext {
    fn enchant_item(
        &self,
        _operation: &super::EnchantmentOperation,
        _stack: ItemStack,
        _params: &LootParams,
        _random: &mut dyn RandomSource,
    ) -> Result<ItemStack> {
        Err(LootError::Unsupported(
            "enchantment.selection_and_application".into(),
        ))
    }
    fn entity_enchantment_level(
        &self,
        _entity: EntityRef,
        _enchantment: &Identifier,
    ) -> Result<i32> {
        Err(LootError::Unsupported("entity.enchantment_level".into()))
    }
    fn smelting_recipe_result(&self, _stack: &ItemStack) -> Result<Option<ItemStack>> {
        Err(LootError::Unsupported("recipes.smelting".into()))
    }
    fn exploration_map(
        &self,
        _options: &super::ExplorationMapOptions,
        _stack: ItemStack,
        _origin: Vec3,
    ) -> Result<ItemStack> {
        Err(LootError::Unsupported("level.exploration_map".into()))
    }
    fn enchantment_provider_order(&self, keys: &[Identifier]) -> Result<Vec<Identifier>> {
        if keys.len() <= 1 {
            Ok(keys.to_vec())
        } else {
            Err(LootError::Unsupported(
                "java.immutable_map_iteration_order".into(),
            ))
        }
    }
    fn environment_float(&self, _attribute: &Identifier, _params: &LootParams) -> Result<f32> {
        Err(LootError::Unsupported("level.environment_attribute".into()))
    }
    fn environment_value(
        &self,
        _attribute: &Identifier,
        _params: &LootParams,
    ) -> Result<super::EnvironmentValue> {
        Err(LootError::Unsupported("level.environment_attribute".into()))
    }
    fn bundle_contents(
        &self,
        _items: Vec<ItemStack>,
    ) -> Result<Vec<ferraloom_protocol::component::ItemStackTemplate>> {
        Err(LootError::Unsupported("inventory.bundle_insertion".into()))
    }
    fn bundle_weight_valid(
        &self,
        _items: &[ferraloom_protocol::component::ItemStackTemplate],
    ) -> Result<bool> {
        Err(LootError::Unsupported("inventory.bundle_weight".into()))
    }
    fn player_profile(
        &self,
        _entity: EntityRef,
    ) -> Result<Option<ferraloom_protocol::component::ResolvableProfile>> {
        Err(LootError::Unsupported("entity.player_profile".into()))
    }
    fn slot_items(
        &self,
        _source: super::SlotTarget,
        _slots: &[i32],
        _params: &LootParams,
    ) -> Result<Vec<ItemStack>> {
        Err(LootError::Unsupported("inventory.slot_provider".into()))
    }
    fn clock_ticks(&self, _clock: &Identifier) -> Result<i64> {
        Err(LootError::Unsupported("level.clock_ticks".into()))
    }
    fn weather(&self) -> Result<(bool, bool)> {
        Err(LootError::Unsupported("level.weather".into()))
    }
    fn score(&self, _holder: ScoreHolder<'_>, _objective: &str) -> Result<Option<i32>> {
        Err(LootError::Unsupported("scoreboard.score".into()))
    }
    fn storage(&self, _id: &Identifier) -> Result<Tag> {
        Err(LootError::Unsupported("command_storage".into()))
    }
    fn dynamic_drops(&self, _id: &Identifier) -> Result<Vec<ItemStack>> {
        Ok(Vec::new())
    }
    fn item_enabled(&self, stack: &ItemStack) -> Result<bool> {
        if stack.is_empty() {
            Ok(true)
        } else {
            Err(LootError::Unsupported("item.required_features".into()))
        }
    }
}
/// All unavailable live facts fail through their default methods.
pub struct NoFacts;
impl PredicateContext for NoFacts {}
impl LootFacts for NoFacts {}

pub(crate) struct EvalContext<'a> {
    pub params: &'a LootParams,
    pub runtime: &'a mut dyn super::LootRuntime,
    pub visited: BTreeSet<(&'static str, Identifier)>,
}
impl EvalContext<'_> {
    pub fn facts(&self) -> &dyn LootFacts {
        self.runtime.facts()
    }
    pub fn random(&mut self) -> super::runtime::RuntimeRandom<'_> {
        super::runtime::RuntimeRandom(self.runtime)
    }
}
