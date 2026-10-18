//! CriteriaTriggers codecs and pure matching. 26.3 uses loot-condition holders
//! directly where older releases used ContextAwarePredicate.
use crate::advancements::optional;
mod matching;
mod types;
use crate::{
    loot::{Holder, HolderSet, Identifier, LootItemCondition, LootTable, Result, codec},
    predicates::{
        BlockPredicate, DamagePredicate, DamageSourcePredicate, DistancePredicate, DoubleBounds,
        EntityPredicate, IntBounds, ItemPredicate, LocationPredicate,
    },
};
use ferraloom_data::{
    Item,
    datapack::{ResourceManager, TagSet},
};
use ferraloom_protocol::component::{MobEffectsPredicate, Persistent, PotionsPredicate};
pub use matching::*;
use serde_json::Value;
pub use types::*;

/// Port of EntityPredicate#wrap and #createContext. These share the loot types.
pub type ContextAwarePredicate = Holder<LootItemCondition>;
pub fn wrap(predicate: EntityPredicate) -> ContextAwarePredicate {
    Holder::direct(LootItemCondition::EntityProperties(
        crate::loot::EntityTarget::This,
        Some(predicate),
    ))
}
pub fn create_context(
    origin: ferraloom_util::Vec3,
    entity: crate::entity::EntityRef,
) -> crate::loot::LootParams {
    crate::loot::LootParams {
        origin: Some(origin),
        this_entity: Some(entity),
        ..Default::default()
    }
}
/// Port of Criterion's trigger/conditions dispatch codec.
#[derive(Clone, Debug, PartialEq)]
pub struct Criterion {
    pub trigger: TriggerType,
    pub instance: TriggerInstance,
}
impl Criterion {
    pub fn decode(v: &Value) -> Result<Self> {
        Self::decode_with_tags(v, &TagSet::<Item>::load(&ResourceManager::new(Vec::new()))?)
    }
    pub fn decode_with_tags(v: &Value, tags: &TagSet<Item>) -> Result<Self> {
        let trigger = TriggerType::from_id(&codec::id(codec::field(v, "trigger")?)?)?;
        let empty = serde_json::json!({});
        let conditions = codec::object(v)?
            .get("conditions")
            .filter(|value| !value.is_null())
            .unwrap_or(&empty);
        Ok(Self {
            trigger,
            instance: TriggerInstance::decode(trigger, conditions, tags)?,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventorySlots {
    pub occupied: IntBounds,
    pub full: IntBounds,
    pub empty: IntBounds,
}
/// Port of every TriggerInstance record. Structurally identical instances share
/// a variant; their registry identities remain distinct in Criterion.trigger.
#[derive(Clone, Debug, PartialEq)]
pub enum TriggerConditions {
    Impossible,
    Player,
    Item(Option<ItemPredicate>),
    Entity(Option<ContextAwarePredicate>),
    Block(BlockPredicate),
    Location(Option<ContextAwarePredicate>),
    Killed {
        entity: Option<ContextAwarePredicate>,
        killing_blow: Option<DamageSourcePredicate>,
    },
    Inventory {
        slots: InventorySlots,
        items: Vec<ItemPredicate>,
    },
    Recipe {
        recipes: HolderSet<crate::crafting::Recipe>,
        ingredients: Vec<ItemPredicate>,
    },
    Hurt {
        damage: Option<DamagePredicate>,
        entity: Option<ContextAwarePredicate>,
    },
    EnchantedItem {
        item: Option<ItemPredicate>,
        levels: IntBounds,
    },
    Potion(Option<PotionsPredicate>),
    Beacon(IntBounds),
    EnderEye(DoubleBounds),
    Bred {
        parent: Option<ContextAwarePredicate>,
        partner: Option<ContextAwarePredicate>,
        child: Option<ContextAwarePredicate>,
    },
    Cured {
        zombie: Option<ContextAwarePredicate>,
        villager: Option<ContextAwarePredicate>,
    },
    Trade {
        villager: Option<ContextAwarePredicate>,
        item: Option<ItemPredicate>,
    },
    Durability {
        item: Option<ItemPredicate>,
        durability: IntBounds,
        delta: IntBounds,
    },
    Levitation {
        distance: Option<DistancePredicate>,
        duration: IntBounds,
    },
    Dimension {
        from: Option<Identifier>,
        to: Option<Identifier>,
    },
    Effects {
        effects: Option<MobEffectsPredicate>,
        source: Option<ContextAwarePredicate>,
    },
    Distance {
        start_position: Option<Box<LocationPredicate>>,
        distance: Option<DistancePredicate>,
        cause: Option<ContextAwarePredicate>,
    },
    Fishing {
        rod: Option<ItemPredicate>,
        entity: Option<ContextAwarePredicate>,
        item: Option<ItemPredicate>,
    },
    ChanneledLightning(Vec<ContextAwarePredicate>),
    SpearMobs(Option<i32>),
    KilledByArrow {
        victims: Vec<ContextAwarePredicate>,
        unique_entity_types: IntBounds,
        fired_from_weapon: Option<ItemPredicate>,
    },
    BeeNest {
        block: BlockPredicate,
        item: Option<ItemPredicate>,
        bees: IntBounds,
    },
    Target {
        signal_strength: IntBounds,
        projectile: Option<ContextAwarePredicate>,
    },
    Loot(HolderSet<LootTable>),
    ItemEntity {
        item: Option<ItemPredicate>,
        entity: Option<ContextAwarePredicate>,
    },
    Lightning {
        lightning: Option<ContextAwarePredicate>,
        bystander: Option<ContextAwarePredicate>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct TriggerInstance {
    pub player: Option<ContextAwarePredicate>,
    pub conditions: TriggerConditions,
}
fn predicate(v: &Value) -> Result<ContextAwarePredicate> {
    Holder::decode(v, LootItemCondition::decode)
}
impl TriggerInstance {
    pub fn decode(trigger: TriggerType, v: &Value, tags: &TagSet<Item>) -> Result<Self> {
        use TriggerConditions as C;
        use TriggerType::*;
        if trigger == Impossible {
            return Ok(Self {
                player: None,
                conditions: C::Impossible,
            });
        }
        codec::object(v)?;
        let pred = |name| optional(v, name, predicate);
        let item = |name| optional(v, name, ItemPredicate::decode);
        let ints = |name| -> Result<IntBounds> {
            Ok(optional(v, name, IntBounds::decode)?.unwrap_or_default())
        };
        let block = || {
            let mut fields = serde_json::Map::new();
            for key in ["blocks", "state"] {
                if let Some(value) = v.get(key).filter(|value| !value.is_null()) {
                    fields.insert(key.into(), value.clone());
                }
            }
            BlockPredicate::decode(&Value::Object(fields))
        };
        let conditions = match trigger {
            Impossible => unreachable!(),
            Location | SleptInBed | Tick | HeroOfTheVillage | VoluntaryExile | StartedRiding
            | AvoidVibration => C::Player,
            ConsumeItem | FilledBucket | UsedTotem | UsingItem | ShotCrossbow => {
                C::Item(item("item")?)
            }
            SummonedEntity | TameAnimal => C::Entity(pred("entity")?),
            EnterBlock | SlideDownBlock => C::Block(block()?),
            PlacedBlock | ItemUsedOnBlock | DefaultBlockUse | AnyBlockUse
            | AllayDropItemOnBlock => C::Location(pred("location")?),
            PlayerKilledEntity | EntityKilledPlayer | KillMobNearSculkCatalyst => C::Killed {
                entity: pred("entity")?,
                killing_blow: optional(v, "killing_blow", DamageSourcePredicate::decode)?,
            },
            InventoryChanged => {
                let empty = serde_json::json!({});
                let slots = v
                    .get("slots")
                    .filter(|value| !value.is_null())
                    .unwrap_or(&empty);
                let bound = |name| -> Result<IntBounds> {
                    Ok(optional(slots, name, IntBounds::decode)?.unwrap_or_default())
                };
                C::Inventory {
                    slots: InventorySlots {
                        occupied: bound("occupied")?,
                        full: bound("full")?,
                        empty: bound("empty")?,
                    },
                    items: optional(v, "items", |v| codec::list(v, ItemPredicate::decode))?
                        .unwrap_or_default(),
                }
            }
            RecipeUnlocked | RecipeCrafted | CrafterRecipeCrafted => C::Recipe {
                recipes: HolderSet::decode(codec::field(v, "recipes")?, |v| {
                    crate::crafting::Recipe::decode(v, tags)
                        .map_err(|e| codec::error(e.to_string()))
                })?,
                ingredients: if trigger == RecipeUnlocked {
                    Vec::new()
                } else {
                    optional(v, "ingredients", |v| codec::list(v, ItemPredicate::decode))?
                        .unwrap_or_default()
                },
            },
            PlayerHurtEntity | EntityHurtPlayer => C::Hurt {
                damage: optional(v, "damage", DamagePredicate::decode)?,
                entity: if trigger == PlayerHurtEntity {
                    pred("entity")?
                } else {
                    None
                },
            },
            EnchantedItem => C::EnchantedItem {
                item: item("item")?,
                levels: ints("levels")?,
            },
            BrewedPotion => C::Potion(optional(v, "potion", |v| {
                Ok(PotionsPredicate::from_json(v)?)
            })?),
            ConstructBeacon => C::Beacon(ints("level")?),
            UsedEnderEye => {
                C::EnderEye(optional(v, "distance", DoubleBounds::decode)?.unwrap_or_default())
            }
            BredAnimals => C::Bred {
                parent: pred("parent")?,
                partner: pred("partner")?,
                child: pred("child")?,
            },
            CuredZombieVillager => C::Cured {
                zombie: pred("zombie")?,
                villager: pred("villager")?,
            },
            VillagerTrade => C::Trade {
                villager: pred("villager")?,
                item: item("item")?,
            },
            ItemDurabilityChanged => C::Durability {
                item: item("item")?,
                durability: ints("durability")?,
                delta: ints("delta")?,
            },
            Levitation => C::Levitation {
                distance: optional(v, "distance", DistancePredicate::decode)?,
                duration: ints("duration")?,
            },
            ChangedDimension => C::Dimension {
                from: optional(v, "from", codec::id)?,
                to: optional(v, "to", codec::id)?,
            },
            EffectsChanged => C::Effects {
                effects: optional(v, "effects", |v| Ok(MobEffectsPredicate::from_json(v)?))?,
                source: pred("source")?,
            },
            NetherTravel | FallFromHeight | RideEntityInLava | FallAfterExplosion => C::Distance {
                start_position: optional(v, "start_position", LocationPredicate::decode)?
                    .map(Box::new),
                distance: optional(v, "distance", DistancePredicate::decode)?,
                cause: if trigger == FallAfterExplosion {
                    pred("cause")?
                } else {
                    None
                },
            },
            FishingRodHooked => C::Fishing {
                rod: item("rod")?,
                entity: pred("entity")?,
                item: item("item")?,
            },
            ChanneledLightning => C::ChanneledLightning(
                optional(v, "victims", |v| codec::list(v, predicate))?.unwrap_or_default(),
            ),
            SpearMobs => {
                let count = optional(v, "count", codec::int)?;
                if count.is_some_and(|c| c <= 0) {
                    return Err(codec::error("count must be positive"));
                }
                C::SpearMobs(count)
            }
            KilledByArrow => C::KilledByArrow {
                victims: optional(v, "victims", |v| codec::list(v, predicate))?.unwrap_or_default(),
                unique_entity_types: ints("unique_entity_types")?,
                fired_from_weapon: item("fired_from_weapon")?,
            },
            BeeNestDestroyed => C::BeeNest {
                block: block()?,
                item: item("item")?,
                bees: ints("num_bees_inside")?,
            },
            TargetHit => C::Target {
                signal_strength: ints("signal_strength")?,
                projectile: pred("projectile")?,
            },
            PlayerGeneratesContainerLoot => C::Loot(HolderSet::decode(
                codec::field(v, "loot_tables")?,
                LootTable::decode,
            )?),
            ThrownItemPickedUpByEntity
            | ThrownItemPickedUpByPlayer
            | PlayerInteractedWithEntity
            | PlayerShearedEquipment => C::ItemEntity {
                item: item("item")?,
                entity: pred("entity")?,
            },
            LightningStrike => C::Lightning {
                lightning: pred("lightning")?,
                bystander: pred("bystander")?,
            },
        };
        Ok(Self {
            player: pred("player")?,
            conditions,
        })
    }
}
/// Registry membership needed for recipe/loot holder tags. The actual holder
/// value of an inline entry has no ResourceKey and never matches a named event.
pub trait TriggerFacts: crate::loot::LootFacts {
    fn recipe_tag(&self, _tag: &Identifier) -> Result<Vec<Identifier>> {
        Err(crate::loot::LootError::Unsupported(
            "advancement.recipe_tag".into(),
        ))
    }
    fn loot_table_tag(&self, _tag: &Identifier) -> Result<Vec<Identifier>> {
        Err(crate::loot::LootError::Unsupported(
            "advancement.loot_table_tag".into(),
        ))
    }
    fn hooked_item(
        &self,
        _entity: crate::entity::EntityRef,
    ) -> Result<Option<ferraloom_protocol::component::ItemStack>> {
        Err(crate::loot::LootError::Unsupported(
            "advancement.fishing_hooked_item".into(),
        ))
    }
}
impl TriggerFacts for crate::loot::NoFacts {}
