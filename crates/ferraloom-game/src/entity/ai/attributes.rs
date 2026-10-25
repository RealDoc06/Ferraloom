//! Canonical attribute values and modifiers, ported from vanilla's attribute package.
mod persistence;
pub use persistence::{AttributeLoadError, PackedAttribute};
use std::{cell::Cell, ops::Deref, sync::Arc};

use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
pub use ferraloom_protocol::component::{AttributeModifier, AttributeOperation};
use ferraloom_protocol::{
    Identifier,
    play::{AttributeId, AttributeSnapshot},
};
use ferraloom_util::collections::{
    JavaHash, Object2ObjectOpenHashMap, ReferenceHashMap, ReferenceKey, ReferenceOpenHashSet,
};

/// Attribute holders and instances have reference identity in Java. A registry
/// ID identifies that same canonical holder; it is never used as a Java hash.
#[derive(Clone, Copy, Debug)]
struct AttributeKey(Attribute);
impl ReferenceKey for AttributeKey {
    fn reference_address(&self) -> usize {
        self.0.protocol_id() as usize + 1
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModifierKey(Identifier);
impl JavaHash for ModifierKey {
    fn java_hash_code(&self) -> i32 {
        self.0
            .namespace()
            .java_hash_code()
            .wrapping_mul(31)
            .wrapping_add(self.0.path().java_hash_code())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributeError {
    MissingAttribute(Attribute),
    DuplicateModifier,
}

impl std::fmt::Display for AttributeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AttributeError {}

/// Port of AttributeInstance. The two array maps retain insertion order, while
/// arithmetic traverses each operation's actual fastutil hash table.
#[derive(Clone, Debug)]
pub struct AttributeInstance {
    attribute: Attribute,
    modifiers_by_operation: [Object2ObjectOpenHashMap<ModifierKey, Arc<AttributeModifier>>; 3],
    modifier_by_id: Vec<Arc<AttributeModifier>>,
    permanent_modifiers: Vec<Arc<AttributeModifier>>,
    base_value: f64,
    dirty: Cell<bool>,
    cached_value: Cell<f64>,
    revision: u64,
}

impl AttributeInstance {
    pub fn new(attribute: Attribute) -> Self {
        Self {
            attribute,
            modifiers_by_operation: std::array::from_fn(|_| Object2ObjectOpenHashMap::new()),
            modifier_by_id: Vec::new(),
            permanent_modifiers: Vec::new(),
            base_value: attribute.data().default_value,
            dirty: Cell::new(true),
            cached_value: Cell::new(0.0),
            revision: 0,
        }
    }
    pub fn attribute(&self) -> Attribute {
        self.attribute
    }
    pub fn base_value(&self) -> f64 {
        self.base_value
    }
    pub fn modifiers(&self) -> &[Arc<AttributeModifier>] {
        &self.modifier_by_id
    }
    pub fn permanent_modifiers(&self) -> &[Arc<AttributeModifier>] {
        &self.permanent_modifiers
    }
    pub fn modifier(&self, id: &Identifier) -> Option<&Arc<AttributeModifier>> {
        self.modifier_by_id
            .iter()
            .find(|modifier| modifier.id == *id)
    }
    pub fn value(&self) -> f64 {
        if self.dirty.replace(false) {
            let mut base = self.base_value;
            for modifier in self.modifiers_by_operation[0].values() {
                base += modifier.amount.0;
            }
            let mut result = base;
            for modifier in self.modifiers_by_operation[1].values() {
                result += base * modifier.amount.0;
            }
            for modifier in self.modifiers_by_operation[2].values() {
                result *= 1.0 + modifier.amount.0;
            }
            self.cached_value.set(sanitize(self.attribute, result));
        }
        self.cached_value.get()
    }
    fn set_dirty(&mut self) {
        self.dirty.set(true);
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn set_base_value(&mut self, value: f64) {
        if value != self.base_value {
            self.base_value = value;
            self.set_dirty();
        }
    }
    fn add_modifier(&mut self, modifier: Arc<AttributeModifier>) -> Result<(), AttributeError> {
        if self.modifier(&modifier.id).is_some() {
            return Err(AttributeError::DuplicateModifier);
        }
        self.modifier_by_id.push(modifier.clone());
        self.modifiers_by_operation[operation_index(modifier.operation)]
            .insert(ModifierKey(modifier.id.clone()), modifier);
        self.set_dirty();
        Ok(())
    }
    pub fn add_transient_modifier(
        &mut self,
        modifier: Arc<AttributeModifier>,
    ) -> Result<(), AttributeError> {
        self.add_modifier(modifier)
    }
    pub fn add_or_update_transient_modifier(&mut self, modifier: Arc<AttributeModifier>) {
        let previous = array_put(&mut self.modifier_by_id, modifier.clone());
        if !previous
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &modifier))
        {
            // Vanilla deliberately leaves the old operation's entry when the
            // same ID is updated to a different operation.
            self.modifiers_by_operation[operation_index(modifier.operation)]
                .insert(ModifierKey(modifier.id.clone()), modifier);
            self.set_dirty();
        }
    }
    pub fn add_permanent_modifier(
        &mut self,
        modifier: Arc<AttributeModifier>,
    ) -> Result<(), AttributeError> {
        self.add_modifier(modifier.clone())?;
        array_put(&mut self.permanent_modifiers, modifier);
        Ok(())
    }
    pub fn add_or_replace_permanent_modifier(&mut self, modifier: Arc<AttributeModifier>) {
        self.remove_modifier(&modifier.id);
        self.add_permanent_modifier(modifier)
            .expect("previous modifier removed");
    }
    pub fn remove_modifier(&mut self, id: &Identifier) -> bool {
        let Some(index) = self
            .modifier_by_id
            .iter()
            .position(|modifier| modifier.id == *id)
        else {
            return false;
        };
        let modifier = self.modifier_by_id.remove(index);
        self.modifiers_by_operation[operation_index(modifier.operation)]
            .remove(&ModifierKey(id.clone()));
        self.permanent_modifiers
            .retain(|modifier| modifier.id != *id);
        self.set_dirty();
        true
    }
    pub fn remove_modifiers(&mut self) {
        for modifier in self.modifier_by_id.clone() {
            self.remove_modifier(&modifier.id);
        }
    }
    pub fn replace_from(&mut self, other: &Self) {
        self.base_value = other.base_value;
        self.modifier_by_id.clone_from(&other.modifier_by_id);
        self.permanent_modifiers
            .clone_from(&other.permanent_modifiers);
        // putAll into fresh operation maps can choose a different table size
        // from cloning the source; replay the native traversal.
        self.modifiers_by_operation = std::array::from_fn(|operation| {
            let mut map = Object2ObjectOpenHashMap::new();
            map.ensure_capacity(other.modifiers_by_operation[operation].len());
            for (id, modifier) in &other.modifiers_by_operation[operation] {
                map.insert(id.clone(), modifier.clone());
            }
            map
        });
        self.set_dirty();
    }
    pub fn snapshot(&self) -> AttributeSnapshot {
        AttributeSnapshot {
            attribute: AttributeId(self.attribute.protocol_id() as i32),
            base: self.base_value,
            modifiers: self
                .modifier_by_id
                .iter()
                .map(|modifier| ferraloom_protocol::play::AttributeModifier {
                    id: modifier.id.clone(),
                    amount: modifier.amount.0,
                    operation: modifier.operation,
                })
                .collect(),
        }
    }
    pub fn pack(&self) -> PackedAttribute {
        PackedAttribute {
            attribute: self.attribute,
            base_value: self.base_value,
            modifiers: self.permanent_modifiers.clone(),
        }
    }
    pub fn apply(&mut self, packed: &PackedAttribute) {
        self.base_value = packed.base_value;
        for modifier in &packed.modifiers {
            array_put(&mut self.modifier_by_id, modifier.clone());
            self.modifiers_by_operation[operation_index(modifier.operation)]
                .insert(ModifierKey(modifier.id.clone()), modifier.clone());
            array_put(&mut self.permanent_modifiers, modifier.clone());
        }
        self.set_dirty();
    }
}

fn operation_index(operation: AttributeOperation) -> usize {
    match operation {
        AttributeOperation::AddValue => 0,
        AttributeOperation::AddMultipliedBase => 1,
        AttributeOperation::AddMultipliedTotal => 2,
    }
}
fn array_put(
    values: &mut Vec<Arc<AttributeModifier>>,
    modifier: Arc<AttributeModifier>,
) -> Option<Arc<AttributeModifier>> {
    if let Some(existing) = values
        .iter_mut()
        .find(|existing| existing.id == modifier.id)
    {
        Some(std::mem::replace(existing, modifier))
    } else {
        values.push(modifier);
        None
    }
}
fn sanitize(attribute: Attribute, value: f64) -> f64 {
    let Some(range) = attribute.data().range else {
        return value;
    };
    if value.is_nan() {
        range.min
    } else {
        value.clamp(range.min, range.max)
    }
}

/// Port of AttributeMap with immutable extracted AttributeSupplier defaults.
/// Identity-key iteration uses the util collection's documented insertion order.
#[derive(Debug)]
pub struct AttributeMap {
    attributes: ReferenceHashMap<AttributeKey, AttributeInstance>,
    attributes_to_sync: ReferenceOpenHashSet<AttributeKey>,
    attributes_to_update: ReferenceOpenHashSet<AttributeKey>,
    supplier: &'static [(Attribute, f64)],
}
impl AttributeMap {
    pub fn new(entity_type: EntityType) -> Self {
        Self::with_supplier(entity_type.data().attributes)
    }
    pub fn with_supplier(supplier: &'static [(Attribute, f64)]) -> Self {
        Self {
            attributes: ReferenceHashMap::new(),
            attributes_to_sync: ReferenceOpenHashSet::new(),
            attributes_to_update: ReferenceOpenHashSet::new(),
            supplier,
        }
    }
    fn default_base(&self, attribute: Attribute) -> Option<f64> {
        self.supplier
            .iter()
            .find_map(|(id, value)| (*id == attribute).then_some(*value))
    }
    pub fn has_attribute(&self, attribute: Attribute) -> bool {
        self.attributes.contains_key(&AttributeKey(attribute))
            || self.default_base(attribute).is_some()
    }
    pub fn value(&self, attribute: Attribute) -> Result<f64, AttributeError> {
        self.attributes
            .get(&AttributeKey(attribute))
            .map(AttributeInstance::value)
            .or_else(|| {
                self.default_base(attribute)
                    .map(|value| sanitize(attribute, value))
            })
            .ok_or(AttributeError::MissingAttribute(attribute))
    }
    pub fn base_value(&self, attribute: Attribute) -> Result<f64, AttributeError> {
        self.attributes
            .get(&AttributeKey(attribute))
            .map(AttributeInstance::base_value)
            .or_else(|| self.default_base(attribute))
            .ok_or(AttributeError::MissingAttribute(attribute))
    }
    pub fn get_instance(&mut self, attribute: Attribute) -> Option<AttributeInstanceMut<'_>> {
        let key = AttributeKey(attribute);
        if !self.attributes.contains_key(&key) {
            let base = self.default_base(attribute)?;
            let mut instance = AttributeInstance::new(attribute);
            instance.base_value = base;
            // AttributeSupplier#createInstance calls replaceFrom even when the
            // default equals the registry default, so creation dirties both sets.
            instance.set_dirty();
            self.attributes.insert(key, instance);
            self.attributes_to_update.insert(key);
            if attribute.data().syncable {
                self.attributes_to_sync.insert(key);
            }
        }
        self.existing_instance_mut(attribute)
    }
    /// Direct AttributeMap#attributes lookup used by removeAttributeModifiers.
    /// Unlike get_instance, absence never installs or dirties a lazy instance.
    pub fn existing_instance_mut(
        &mut self,
        attribute: Attribute,
    ) -> Option<AttributeInstanceMut<'_>> {
        Some(AttributeInstanceMut {
            instance: self.attributes.get_mut(&AttributeKey(attribute))?,
            to_sync: &mut self.attributes_to_sync,
            to_update: &mut self.attributes_to_update,
        })
    }
    pub fn has_modifier(&self, attribute: Attribute, id: &Identifier) -> bool {
        self.attributes
            .get(&AttributeKey(attribute))
            .is_some_and(|instance| instance.modifier(id).is_some())
    }
    pub fn syncable_attributes(&self) -> impl Iterator<Item = &AttributeInstance> {
        self.attributes
            .values()
            .filter(|instance| instance.attribute.data().syncable)
    }
    pub fn attributes_to_sync(&self) -> impl Iterator<Item = &AttributeInstance> {
        self.attributes_to_sync
            .iter()
            .map(|key| self.attributes.get(key).unwrap())
    }
    pub fn attributes_to_update(&self) -> impl Iterator<Item = &AttributeInstance> {
        self.attributes_to_update
            .iter()
            .map(|key| self.attributes.get(key).unwrap())
    }
    pub fn clear_attributes_to_sync(&mut self) {
        self.attributes_to_sync.clear();
    }
    pub fn clear_attributes_to_update(&mut self) {
        self.attributes_to_update.clear();
    }
    pub fn reset_base_value(&mut self, attribute: Attribute) -> bool {
        let Some(value) = self.default_base(attribute) else {
            return false;
        };
        if self.attributes.contains_key(&AttributeKey(attribute)) {
            self.get_instance(attribute).unwrap().set_base_value(value);
        }
        true
    }
    pub fn pack(&self) -> Vec<PackedAttribute> {
        self.attributes
            .values()
            .map(AttributeInstance::pack)
            .collect()
    }
    pub fn apply(&mut self, packed: &[PackedAttribute]) {
        for attribute in packed {
            if let Some(mut instance) = self.get_instance(attribute.attribute) {
                instance.apply(attribute);
            }
        }
    }
}

/// A short mutable instance borrow also owns its AttributeMap callback context.
/// There is no mutable projection that can bypass native dirty-set updates.
pub struct AttributeInstanceMut<'a> {
    instance: &'a mut AttributeInstance,
    to_sync: &'a mut ReferenceOpenHashSet<AttributeKey>,
    to_update: &'a mut ReferenceOpenHashSet<AttributeKey>,
}
impl Deref for AttributeInstanceMut<'_> {
    type Target = AttributeInstance;
    fn deref(&self) -> &Self::Target {
        self.instance
    }
}
impl AttributeInstanceMut<'_> {
    fn mutate<R>(&mut self, update: impl FnOnce(&mut AttributeInstance) -> R) -> R {
        let before = self.instance.revision;
        let result = update(self.instance);
        if self.instance.revision != before {
            let key = AttributeKey(self.instance.attribute);
            self.to_update.insert(key);
            if key.0.data().syncable {
                self.to_sync.insert(key);
            }
        }
        result
    }
    pub fn set_base_value(&mut self, value: f64) {
        self.mutate(|instance| instance.set_base_value(value));
    }
    pub fn add_transient_modifier(
        &mut self,
        modifier: Arc<AttributeModifier>,
    ) -> Result<(), AttributeError> {
        self.mutate(|instance| instance.add_transient_modifier(modifier))
    }
    pub fn add_or_update_transient_modifier(&mut self, modifier: Arc<AttributeModifier>) {
        self.mutate(|instance| instance.add_or_update_transient_modifier(modifier));
    }
    pub fn add_permanent_modifier(
        &mut self,
        modifier: Arc<AttributeModifier>,
    ) -> Result<(), AttributeError> {
        self.mutate(|instance| instance.add_permanent_modifier(modifier))
    }
    pub fn add_or_replace_permanent_modifier(&mut self, modifier: Arc<AttributeModifier>) {
        self.mutate(|instance| instance.add_or_replace_permanent_modifier(modifier));
    }
    pub fn remove_modifier(&mut self, id: &Identifier) -> bool {
        self.mutate(|instance| instance.remove_modifier(id))
    }
    pub fn remove_modifiers(&mut self) {
        self.mutate(AttributeInstance::remove_modifiers);
    }
    pub fn replace_from(&mut self, other: &AttributeInstance) {
        self.mutate(|instance| instance.replace_from(other));
    }
    pub fn apply(&mut self, packed: &PackedAttribute) {
        self.mutate(|instance| instance.apply(packed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifier(id: &str, amount: f64, operation: AttributeOperation) -> Arc<AttributeModifier> {
        Arc::new(AttributeModifier {
            id: Identifier::parse(id).unwrap(),
            amount: amount.into(),
            operation,
        })
    }
    fn emit(rows: &mut Vec<String>, name: &str, instance: &AttributeInstance, callbacks: u64) {
        let ids = |values: &[Arc<AttributeModifier>]| {
            values
                .iter()
                .map(|modifier| modifier.id.path())
                .collect::<Vec<_>>()
                .join(",")
        };
        rows.push(format!(
            "attribute\t{name}\t{:x}\t{:x}\t{}\t{}\t{callbacks}",
            instance.base_value().to_bits(),
            instance.value().to_bits(),
            ids(instance.modifiers()),
            ids(instance.permanent_modifiers())
        ));
    }
    #[test]
    fn real_attribute_instances_preserve_hash_arithmetic_identity_and_callbacks() {
        use AttributeOperation::{AddMultipliedBase, AddMultipliedTotal, AddValue};
        let mut rows = Vec::new();
        let mut instance = AttributeInstance::new(Attribute::MOVEMENT_SPEED);
        emit(&mut rows, "default", &instance, instance.revision);
        instance.set_base_value(0.125);
        emit(&mut rows, "base", &instance, instance.revision);
        instance
            .add_transient_modifier(modifier("first", 0.2, AddValue))
            .unwrap();
        instance
            .add_permanent_modifier(modifier("base", 0.3, AddMultipliedBase))
            .unwrap();
        instance
            .add_permanent_modifier(modifier("total", 0.4, AddMultipliedTotal))
            .unwrap();
        emit(&mut rows, "three-operations", &instance, instance.revision);
        let same = modifier("first", 0.5, AddMultipliedTotal);
        instance.add_or_update_transient_modifier(same.clone());
        emit(&mut rows, "changed-operation", &instance, instance.revision);
        instance.add_or_update_transient_modifier(same.clone());
        emit(&mut rows, "same-object", &instance, instance.revision);
        instance.add_or_update_transient_modifier(modifier("first", 0.5, AddMultipliedTotal));
        emit(&mut rows, "equal-new-object", &instance, instance.revision);
        instance.remove_modifier(&same.id);
        emit(
            &mut rows,
            "remove-changed-operation",
            &instance,
            instance.revision,
        );
        instance.remove_modifiers();
        emit(
            &mut rows,
            "remove-all-stale-operation",
            &instance,
            instance.revision,
        );
        for i in 0..40 {
            let amount = match i % 4 {
                0 => 1e8,
                1 => -1e8,
                _ => f64::from(i) / 100.0,
            };
            instance
                .add_permanent_modifier(modifier(&format!("entry_{i}"), amount, AddValue))
                .unwrap();
        }
        emit(&mut rows, "rehash", &instance, instance.revision);
        let mut large_copy = AttributeInstance::new(Attribute::MOVEMENT_SPEED);
        large_copy.replace_from(&instance);
        emit(
            &mut rows,
            "replace-large",
            &large_copy,
            instance.revision + large_copy.revision,
        );
        for i in 0..32 {
            instance.remove_modifier(&Identifier::parse(&format!("entry_{i}")).unwrap());
        }
        emit(
            &mut rows,
            "shrink",
            &instance,
            instance.revision + large_copy.revision,
        );
        let mut copy = AttributeInstance::new(Attribute::MOVEMENT_SPEED);
        copy.replace_from(&instance);
        emit(
            &mut rows,
            "replace",
            &copy,
            instance.revision + large_copy.revision + copy.revision,
        );
        for (name, value) in [
            ("nan", f64::NAN),
            ("positive-infinity", f64::INFINITY),
            ("negative-infinity", f64::NEG_INFINITY),
        ] {
            instance.set_base_value(value);
            emit(
                &mut rows,
                name,
                &instance,
                instance.revision + large_copy.revision + copy.revision,
            );
        }
        let mut gravity = AttributeInstance::new(Attribute::GRAVITY);
        gravity.set_base_value(-0.0);
        emit(
            &mut rows,
            "negative-zero",
            &gravity,
            instance.revision + large_copy.revision + copy.revision + gravity.revision,
        );
        gravity.set_base_value(0.0);
        emit(
            &mut rows,
            "equal-positive-zero",
            &gravity,
            instance.revision + large_copy.revision + copy.revision + gravity.revision,
        );
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("attribute\t"))
            .collect();
        assert_eq!(rows, expected);
    }

    #[test]
    fn lazy_instances_and_mutations_use_the_native_dirty_sets() {
        let mut map = AttributeMap::new(EntityType::PLAYER);
        let mut rows = Vec::new();
        let emit = |rows: &mut Vec<String>, name: &str, map: &AttributeMap| {
            rows.push(format!(
                "attribute-map\t{name}\t{}\t{}\t{}",
                map.attributes_to_sync().count(),
                map.attributes_to_update().count(),
                map.syncable_attributes().count()
            ))
        };
        emit(&mut rows, "fresh", &map);
        map.get_instance(Attribute::MAX_HEALTH).unwrap();
        emit(&mut rows, "created", &map);
        map.clear_attributes_to_sync();
        map.clear_attributes_to_update();
        map.get_instance(Attribute::MAX_HEALTH)
            .unwrap()
            .set_base_value(20.0);
        emit(&mut rows, "unchanged", &map);
        map.get_instance(Attribute::MAX_HEALTH)
            .unwrap()
            .set_base_value(30.0);
        emit(&mut rows, "modified", &map);
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("attribute-map\t"))
            .collect();
        assert_eq!(rows, expected);
        assert!(!map.has_attribute(Attribute::FOLLOW_RANGE));
        assert!(map.get_instance(Attribute::FOLLOW_RANGE).is_none());
        assert_eq!(
            map.value(Attribute::MOVEMENT_SPEED).unwrap().to_bits(),
            f64::from(0.1_f32).to_bits()
        );
    }

    #[test]
    fn native_modifier_removal_only_visits_existing_instances_and_preserves_saved_attributes() {
        let mut cases = 0;
        for row in include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("attribute-removal\t"))
        {
            let fields: Vec<_> = row.split('\t').collect();
            let scenario = fields[1];
            let mut map = AttributeMap::new(EntityType::PLAYER);
            let attribute = if scenario == "unsupported" {
                Attribute::FOLLOW_RANGE
            } else {
                Attribute::SUBMERGED_MINING_SPEED
            };
            let removed = modifier("minecraft:remove", 0.5, AttributeOperation::AddValue);
            if !matches!(scenario, "absent" | "unsupported") {
                let mut instance = map.get_instance(attribute).unwrap();
                if matches!(scenario, "transient" | "mixed") {
                    instance.add_transient_modifier(removed.clone()).unwrap();
                }
                if scenario == "permanent" {
                    instance.add_permanent_modifier(removed.clone()).unwrap();
                }
                if matches!(scenario, "different-id" | "mixed") {
                    instance
                        .add_permanent_modifier(modifier(
                            "minecraft:keep",
                            0.25,
                            AttributeOperation::AddValue,
                        ))
                        .unwrap();
                }
            }
            map.clear_attributes_to_sync();
            map.clear_attributes_to_update();
            if let Some(mut instance) = map.existing_instance_mut(attribute) {
                instance.remove_modifier(&removed.id);
            }
            assert_eq!(
                map.save_nbt(),
                ferraloom_nbt::parse(fields[2]).unwrap(),
                "{row}",
            );
            assert_eq!(
                map.attributes_to_sync().count(),
                fields[3].parse::<usize>().unwrap(),
                "{row}",
            );
            assert_eq!(
                map.attributes_to_update().count(),
                fields[4].parse::<usize>().unwrap(),
                "{row}",
            );
            cases += 1;
        }
        assert_eq!(cases, 7);
    }
}
