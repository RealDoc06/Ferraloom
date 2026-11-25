//! The attribute instances a server syncs for the living entities it tracks
//! (`ClientboundUpdateAttributesPacket`, lan/attributes.rs): the ones
//! vanilla's `AttributeMap` would have created by then, since only those are
//! sent, never the type's other defaults.
use super::Game;
use super::commands::Target;
use horizon_client::{
    effects,
    items::ItemStack,
    lan::attributes::{add_modifier, generic_syncable, instance, operation_named},
    mobs::{
        Mob,
        finalize::{self, Attr, Op},
    },
    motion::PlayerMotion,
};
use horizon_protocol::packets::play::{AttributeModifier, AttributeSnapshot};
use std::sync::Arc;

/// `EquipmentSlotGroup.test`: whether a modifier's slot group covers a slot.
fn slot_in_group(group: &str, slot: &str) -> bool {
    group == slot
        || group == "any"
        || (group == "hand" && matches!(slot, "mainhand" | "offhand"))
        || (group == "armor" && matches!(slot, "feet" | "legs" | "chest" | "head" | "body"))
}

/// A player's `EquipmentSlot`s in `EquipmentSlot.VALUES` order, as
/// `PlayerMotion::last_equipment` holds them.
const PLAYER_SLOTS: [&str; 6] = ["mainhand", "offhand", "feet", "legs", "chest", "head"];

/// `ItemStack.forEachModifier(EquipmentSlot)`: the item's own modifiers for
/// `slot`, then those of its enchantments whose slots cover it, each id
/// suffixed with the slot (`EnchantmentAttributeEffect.idForSlot`), as
/// (attribute, id, amount, operation).
fn stack_modifiers(
    items: &horizon_client::items::Items,
    enchantments: &horizon_client::enchant::Enchantments,
    stack: &ItemStack,
    slot: &str,
) -> Vec<(String, String, f64, String)> {
    let mut out: Vec<(String, String, f64, String)> = items
        .get(&stack.item)
        .into_iter()
        .flat_map(|info| &info.modifiers)
        .filter(|m| slot_in_group(&m.4, slot))
        .map(|(attribute, id, amount, op, _)| (attribute.clone(), id.clone(), *amount, op.clone()))
        .collect();
    // One row per slot group an enchantment lists: two groups covering the
    // slot are one match (`Enchantment.matchingSlot`).
    for (attribute, id, amount, op, group) in enchantments.modifier_rows(stack) {
        let id = format!("{id}/{slot}");
        if slot_in_group(&group, slot) && !out.iter().any(|m| m.0 == attribute && m.1 == id) {
            out.push((attribute, id, amount, op));
        }
    }
    out
}

/// Whether `apply_equipment` would leave the body as it is: its last stacks
/// are `equipment` (in `PLAYER_SLOTS` order).
pub(super) fn wears(body: &PlayerMotion, equipment: [Option<&ItemStack>; 6]) -> bool {
    body.last_equipment
        .iter()
        .zip(equipment)
        .all(|(last, now)| last.as_ref() == now)
}

impl Game<'_> {
    /// The attribute part of `LivingEntity.detectEquipmentUpdates`
    /// (`collectEquipmentChanges`) for the player: each slot whose stack no
    /// longer matches the last one (`ItemStack.matches`) takes the old
    /// stack's modifiers off, then each changed slot's new stack puts its own
    /// on, the main hand's attack damage and speed included. Only changed
    /// slots touch the map, so only their attributes are synced
    /// (`getAttributesToSync`): the client's attack indicator reads the
    /// attack speed it is sent. A diff of six stacks when nothing changed;
    /// the last stacks live on the body (`PlayerMotion::last_equipment`), so
    /// a new one gets everything put on again.
    pub(super) fn apply_equipment(&mut self) {
        let current = [
            self.inventory.get(self.selected).and_then(Option::as_ref),
            self.offhand.as_ref(),
            self.armor[3].as_ref(),
            self.armor[2].as_ref(),
            self.armor[1].as_ref(),
            self.armor[0].as_ref(),
        ];
        let Some(player) = self.player.as_mut() else {
            return;
        };
        // Empty (no allocation) on the usual tick, when nothing changed.
        let changed: Vec<usize> = (0..PLAYER_SLOTS.len())
            .filter(|&i| current[i] != player.last_equipment[i].as_ref())
            .collect();
        let (items, enchantments) = (self.res.items(), &self.res.stations().enchantments);
        let modifiers = |stack: Option<&ItemStack>, i: usize| {
            stack.map_or_else(Vec::new, |s| {
                stack_modifiers(items, enchantments, s, PLAYER_SLOTS[i])
            })
        };
        for &i in &changed {
            for (attribute, id, _, op) in modifiers(player.last_equipment[i].as_ref(), i) {
                let _ = player.set_modifier(&attribute, &id, 0.0, &op, false);
            }
        }
        for &i in &changed {
            for (attribute, id, amount, op) in modifiers(current[i], i) {
                let _ = player.set_modifier(&attribute, &id, amount, &op, true);
            }
            player.last_equipment[i] = current[i].cloned();
        }
    }

    /// `getAttributeValue(KNOCKBACK_RESISTANCE)` of a mob: its own value plus
    /// the modifiers of the items it wears or holds in matching slots.
    pub(super) fn mob_knockback_resistance(&self, m: &Mob) -> f64 {
        let e = &m.extra;
        let slots = [
            ("mainhand", &e.main_hand),
            ("offhand", &e.off_hand),
            ("feet", &e.feet),
            ("legs", &e.legs),
            ("chest", &e.chest),
            ("head", &e.head),
        ];
        let mut extra = Vec::new();
        for (slot, item) in slots {
            let Some(info) = item.as_ref().and_then(|i| self.res.items().get(i)) else {
                continue;
            };
            for (attribute, _, amount, op, group) in &info.modifiers {
                if attribute == "minecraft:knockback_resistance" && slot_in_group(group, slot) {
                    let op = match operation_named(op) {
                        1 => Op::AddMultipliedBase,
                        2 => Op::AddMultipliedTotal,
                        _ => Op::AddValue,
                    };
                    extra.push((*amount, op));
                }
            }
        }
        finalize::value_with(m, Attr::KnockbackResistance, &extra)
    }

    /// What the synced attribute instances of the entities are made from,
    /// borrowed so a pass on other threads can read it (`lan_entities`).
    pub(super) fn attribute_sources(&self) -> AttributeSources<'_> {
        AttributeSources {
            catalog: &self.motion.attributes,
            items: self.res.items(),
            generic: &self.cmds.attributes,
        }
    }

    /// The instances a server player's tick creates, and
    /// `ServerPlayer.updatePlayerAttributes`: a creative player reaches 0.5
    /// further to blocks and 2 to entities. Its client reads the reach from
    /// these synced modifiers. (The crouching player's waypoint range is
    /// server-side only: lan_waypoints.rs hides it.)
    pub(super) fn update_player_attributes(&mut self) {
        let creative = self.mode_id() == 1;
        let Some(p) = self.player.as_mut() else {
            return;
        };
        // LivingEntity.aiStep's removeFrost reads the movement speed instance
        // every tick, which creates it (and its first sync).
        if let Some(key) = p.attributes.key("minecraft:movement_speed") {
            let _ = p.attributes.get_instance(Some(key));
        }
        for (attribute, id, amount) in [
            (
                "minecraft:block_interaction_range",
                "minecraft:creative_mode_block_range",
                0.5,
            ),
            (
                "minecraft:entity_interaction_range",
                "minecraft:creative_mode_entity_range",
                2.0,
            ),
        ] {
            let Some(key) = p.attributes.key(attribute) else {
                continue;
            };
            // getAttribute creates the instance, a creative player's or not.
            if !matches!(p.attributes.get_instance(Some(key.clone())), Ok(Some(_))) {
                continue;
            }
            // addOrUpdateTransientModifier with the same modifier and
            // removeModifier of a missing one leave the instance clean.
            let has = p.attributes.has_modifier(Some(key), id).unwrap_or(false);
            if has != creative {
                let _ = p.set_modifier(attribute, id, amount, "add_value", creative);
            }
        }
    }
}

/// The attribute catalog, the items' modifiers and the instances
/// `/attribute` made (`Game::attribute_sources`).
pub(super) struct AttributeSources<'a> {
    catalog: &'a horizon_world::attributes::entity::Catalog,
    items: &'a horizon_client::items::Items,
    generic:
        &'a std::collections::HashMap<Target, horizon_world::attributes::entity::EntityAttributes>,
}

impl AttributeSources<'_> {
    /// The registry id of a client-syncable attribute the entity type has
    /// (`AttributeMap.getInstance` is null for the others), by name.
    fn synced_attribute(&self, entity: &str, name: &str) -> Option<i32> {
        let catalog = self.catalog;
        let id = catalog.id(name)?;
        (catalog.attribute(id).syncable && catalog.default_base(entity, id).is_some())
            .then(|| id.registry_id())
    }

    /// The type's base value of an attribute, as the instance starts.
    fn attribute_default(&self, entity: &str, name: &str) -> f64 {
        let catalog = self.catalog;
        catalog
            .id(name)
            .and_then(|id| catalog.default_base(entity, id))
            .unwrap_or(0.0)
    }

    /// `LivingEntity.collectEquipmentChanges`: each worn or held item's
    /// attribute modifiers, slot by slot in `EquipmentSlot` order, create
    /// their instances (an instance without a modifier keeps its base).
    fn equipment_attributes<'a>(
        &self,
        entity: &str,
        base: impl Fn(&str) -> Option<f64>,
        slots: impl IntoIterator<Item = (&'static str, Option<&'a Arc<str>>)>,
        out: &mut Vec<AttributeSnapshot>,
    ) {
        for (slot, item) in slots {
            let Some(info) = item.and_then(|i| self.items.get(i)) else {
                continue;
            };
            for (attribute, id, amount, op, group) in &info.modifiers {
                if !slot_in_group(group, slot) {
                    continue;
                }
                let Some(a) = self.synced_attribute(entity, attribute) else {
                    continue;
                };
                let start =
                    base(attribute).unwrap_or_else(|| self.attribute_default(entity, attribute));
                add_modifier(
                    out,
                    a,
                    start,
                    AttributeModifier {
                        id: id.clone(),
                        amount: *amount,
                        operation: operation_named(op),
                    },
                );
            }
        }
    }

    /// A mob's synced instances: movement speed (its first `aiStep` reads it,
    /// `LivingEntity.removeFrost`), what finalizeSpawn set, the baby speed
    /// modifier, its effects' modifiers, its equipment's, and what
    /// `/attribute` changed.
    pub(super) fn mob_attributes(&self, m: &Mob, out: &mut Vec<AttributeSnapshot>) {
        let kind = m.kind.id.as_str();
        if let Some(a) = self.synced_attribute(kind, "minecraft:movement_speed") {
            instance(out, a, finalize::base_value(m, Attr::MovementSpeed));
        }
        if let Some(s) = m.extra.attributes.as_deref() {
            for (attr, v) in &s.bases {
                let synced =
                    super::stats::namespaced(attr.name(), |name| self.synced_attribute(kind, name));
                if let Some(a) = synced {
                    instance(out, a, *v).base = *v;
                }
            }
            for x in &s.modifiers {
                let synced = super::stats::namespaced(x.attr.name(), |name| {
                    self.synced_attribute(kind, name)
                });
                if let Some(a) = synced {
                    add_modifier(
                        out,
                        a,
                        finalize::base_value(m, x.attr),
                        AttributeModifier {
                            id: x.id.to_owned(),
                            amount: x.amount,
                            operation: match x.op {
                                Op::AddValue => 0,
                                Op::AddMultipliedBase => 1,
                                Op::AddMultipliedTotal => 2,
                            },
                        },
                    );
                }
            }
        }
        if let (Some(a), Some((amount, Op::AddMultipliedBase))) = (
            self.synced_attribute(kind, "minecraft:movement_speed"),
            finalize::transient(m, Attr::MovementSpeed),
        ) {
            add_modifier(
                out,
                a,
                finalize::base_value(m, Attr::MovementSpeed),
                AttributeModifier {
                    id: "minecraft:baby".into(),
                    amount,
                    operation: 1,
                },
            );
        }
        let base = |name: &str| {
            Attr::ALL
                .into_iter()
                .find(|a| name.strip_prefix("minecraft:") == Some(a.name()))
                .map(|a| finalize::base_value(m, a))
        };
        // LivingEntity.onEffectAdded: MobEffect.addAttributeModifiers.
        for &(effect, _, amplifier) in &m.extra.effects {
            let Some((attribute, id, amount, op)) =
                effects::effect_modifier(effect.trim_start_matches("minecraft:"))
            else {
                continue;
            };
            let Some(a) = self.synced_attribute(kind, attribute) else {
                continue;
            };
            let start = base(attribute).unwrap_or_else(|| self.attribute_default(kind, attribute));
            add_modifier(
                out,
                a,
                start,
                AttributeModifier {
                    id: id.to_owned(),
                    amount: amount * f64::from(amplifier + 1),
                    operation: operation_named(op),
                },
            );
        }
        let e = &m.extra;
        self.equipment_attributes(
            kind,
            base,
            [
                ("mainhand", e.main_hand.as_ref()),
                ("offhand", e.off_hand.as_ref()),
                ("feet", e.feet.as_ref()),
                ("legs", e.legs.as_ref()),
                ("chest", e.chest.as_ref()),
                ("head", e.head.as_ref()),
            ],
            out,
        );
        self.generic_attributes_of(Target::Mob(m.id), out);
    }

    /// An armor stand's synced instances: movement speed (its first
    /// `aiStep`), its equipment's modifiers and what `/attribute` changed.
    pub(super) fn stand_attributes(
        &self,
        id: u32,
        equipment: &[Option<horizon_client::items::ItemStack>; 6],
    ) -> Vec<AttributeSnapshot> {
        const KIND: &str = "minecraft:armor_stand";
        let mut out = Vec::new();
        if let Some(a) = self.synced_attribute(KIND, "minecraft:movement_speed") {
            instance(
                &mut out,
                a,
                self.attribute_default(KIND, "minecraft:movement_speed"),
            );
        }
        let slots = horizon_client::entities::ArmorStand::SLOTS
            .into_iter()
            .zip(equipment.iter().map(|s| s.as_ref().map(|s| &s.item)));
        self.equipment_attributes(KIND, |_| None, slots, &mut out);
        self.generic_attributes_of(Target::Stand(id), &mut out);
        out
    }

    /// The instances `/attribute` created in an entity's generic map
    /// replace the ones derived from its state.
    fn generic_attributes_of(&self, t: Target, out: &mut Vec<AttributeSnapshot>) {
        let Some(map) = self.generic.get(&t) else {
            return;
        };
        for s in generic_syncable(map) {
            let (attribute, base) = (s.attribute, s.base);
            *instance(out, attribute, base) = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::slot_in_group;

    #[test]
    fn slot_groups_cover_their_slots() {
        assert!(slot_in_group("head", "head"));
        assert!(slot_in_group("armor", "chest"));
        assert!(!slot_in_group("armor", "mainhand"));
        assert!(slot_in_group("hand", "offhand"));
        assert!(slot_in_group("any", "feet"));
        assert!(!slot_in_group("mainhand", "offhand"));
    }
}
