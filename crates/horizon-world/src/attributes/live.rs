//! Registered entity attributes backed by live native-order maps and queues.
use super::{
    AttributeError, Instance, Modifier, Observer,
    entity::{AttributeId, Catalog},
};
use crate::{
    attribute_map::{AttributeMap, ModifierValueError, QueryWorld, ValueKind},
    effect_attribute_modifiers::{self as effects, Definitions},
    object_open_set::{ObjectKey, ObjectOpenSet},
    stored_effect_attributes::{self as stored, AttributeAccess},
};
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Supplies instance hashes without requiring a callback in canonical entities.
/// Existing differential bindings may still supply a closure.
pub trait IdentitySource {
    fn next_identity(&mut self) -> i32;
}
impl<F: FnMut() -> i32> IdentitySource for F {
    fn next_identity(&mut self) -> i32 {
        self()
    }
}

/// The resident player's wrapping identity counter, or an exact oracle replay.
/// Both forms own their sequence and can move with the player to a worker.
pub struct IdentityCounter(CounterState);
enum CounterState {
    Wrapping(i32),
    Recorded {
        values: std::vec::IntoIter<i32>,
        consumed: Arc<AtomicUsize>,
    },
}
impl Default for IdentityCounter {
    fn default() -> Self {
        Self(CounterState::Wrapping(0))
    }
}
impl IdentityCounter {
    /// Preserve the oracle's values and expose its consumed-count assertion.
    pub fn recorded(values: Vec<i32>) -> (Self, Arc<AtomicUsize>) {
        let consumed = Arc::new(AtomicUsize::new(0));
        (
            Self(CounterState::Recorded {
                values: values.into_iter(),
                consumed: Arc::clone(&consumed),
            }),
            consumed,
        )
    }
}
impl IdentitySource for IdentityCounter {
    fn next_identity(&mut self) -> i32 {
        match &mut self.0 {
            CounterState::Wrapping(last) => {
                *last = last.wrapping_add(1);
                *last
            }
            CounterState::Recorded { values, consumed } => {
                consumed.fetch_add(1, Ordering::Relaxed);
                values.next().expect("unexpected instance allocation")
            }
        }
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn counter_keeps_the_wrapping_sequence_and_exact_replay() {
        let mut counter = IdentityCounter::default();
        assert_eq!([counter.next_identity(), counter.next_identity()], [1, 2]);
        let mut wrapping = IdentityCounter(CounterState::Wrapping(i32::MAX));
        assert_eq!(wrapping.next_identity(), i32::MIN);
        assert_eq!(wrapping.next_identity(), i32::MIN + 1);

        let (mut recorded, consumed) = IdentityCounter::recorded(vec![42, -7, 42]);
        assert_eq!(
            std::array::from_fn::<_, 3, _>(|_| recorded.next_identity()),
            [42, -7, 42]
        );
        assert_eq!(consumed.load(Ordering::Relaxed), 3);
    }
}

#[derive(Clone)]
pub struct Key {
    catalog: Arc<Catalog>,
    id: AttributeId,
    hash: i32,
}
impl Key {
    pub fn attribute(&self) -> AttributeId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.catalog.attribute(self.id).name
    }
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.catalog, &other.catalog) && self.id == other.id
    }
}
impl Eq for Key {}
impl ObjectKey for Key {
    fn object_hash(&self) -> i32 {
        self.hash
    }
}
#[derive(Clone)]
pub struct Handle {
    owner: Arc<()>,
    index: usize,
    hash: i32,
}
impl Handle {
    pub fn index(&self) -> usize {
        self.index
    }
}
impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.index == other.index
    }
}
impl Eq for Handle {}
impl ObjectKey for Handle {
    fn object_hash(&self) -> i32 {
        self.hash
    }
}
#[derive(Debug)]
pub enum Error {
    UnknownEntity,
    InvalidHolderHashes,
    NullAttribute,
    MissingAttribute,
    MissingModifier,
    MissingInstanceModifier,
    ForeignInstance,
    DuplicateModifier(String),
}
struct Node {
    handle: Handle,
    key: Key,
    value: Instance,
}
/// Observer for registered attributes. Registry syncability is immutable for
/// this binding; richer virtual attributes can use AttributeMap directly.
pub struct Dirty<'a> {
    map: &'a mut AttributeMap<Key, Handle>,
    handle: Handle,
    syncable: bool,
}
impl Observer for Dirty<'_> {
    type Error = Infallible;
    fn dirty(&mut self, _: &mut Instance) -> Result<(), Infallible> {
        self.map
            .modified(Some(self.handle.clone()), |_, _| Ok(self.syncable))
    }
}
struct Data<G> {
    catalog: Arc<Catalog>,
    bases: Arc<[Option<f64>]>,
    keys: Vec<Key>,
    owner: Arc<()>,
    nodes: Vec<Node>,
    next_identity: G,
}
impl<G: IdentitySource> Data<G> {
    fn base(&self, key: &Key) -> Option<f64> {
        Arc::ptr_eq(&self.catalog, &key.catalog)
            .then(|| self.bases[key.id.index()])
            .flatten()
    }
    fn node(&self, h: &Handle) -> Result<&Node, Error> {
        if !Arc::ptr_eq(&self.owner, &h.owner) {
            return Err(Error::ForeignInstance);
        }
        self.nodes.get(h.index).ok_or(Error::ForeignInstance)
    }
    fn node_mut(&mut self, h: &Handle) -> Result<&mut Node, Error> {
        if !Arc::ptr_eq(&self.owner, &h.owner) {
            return Err(Error::ForeignInstance);
        }
        self.nodes.get_mut(h.index).ok_or(Error::ForeignInstance)
    }
    fn create(
        &mut self,
        map: &mut AttributeMap<Key, Handle>,
        key: Option<Key>,
    ) -> Result<Option<Handle>, Error> {
        let Some(key) = key else {
            return Ok(None);
        };
        let Some(base) = self.base(&key) else {
            return Ok(None);
        };
        let definition = self.catalog.attribute(key.id);
        let mut source = Instance::new(definition.definition);
        // Registered suppliers have no modifiers. Preserve exported base bits
        // rather than invoking an equality-short-circuiting setter here.
        source.base = base;
        let mut value = Instance::new(definition.definition);
        let handle = Handle {
            owner: self.owner.clone(),
            index: self.nodes.len(),
            hash: self.next_identity.next_identity(),
        };
        let mut observer = Dirty {
            map,
            handle: handle.clone(),
            syncable: definition.syncable,
        };
        value
            .replace_from(&source, &mut observer)
            .map_err(convert_error)?;
        self.nodes.push(Node {
            key,
            handle: handle.clone(),
            value,
        });
        Ok(Some(handle))
    }
    fn edit<R>(
        &mut self,
        map: &mut AttributeMap<Key, Handle>,
        handle: &Handle,
        edit: impl FnOnce(&mut Instance, &mut Dirty<'_>) -> Result<R, AttributeError<Infallible>>,
    ) -> Result<R, Error> {
        let node = self.node_mut(handle)?;
        let syncable = node.key.catalog.attribute(node.key.id).syncable;
        let mut observer = Dirty {
            map,
            handle: handle.clone(),
            syncable,
        };
        edit(&mut node.value, &mut observer).map_err(convert_error)
    }
}
fn convert_error(error: AttributeError<Infallible>) -> Error {
    match error {
        AttributeError::DuplicateModifier(id) => Error::DuplicateModifier(id),
        AttributeError::Observer(e) => match e {},
    }
}
impl<G: IdentitySource> QueryWorld<Key, Handle> for Data<G> {
    type Error = Error;
    fn supplier_has_attribute(
        &mut self,
        _: &mut AttributeMap<Key, Handle>,
        key: Option<Key>,
    ) -> Result<bool, Error> {
        Ok(key.is_some_and(|k| self.base(&k).is_some()))
    }
    fn supplier_has_modifier(
        &mut self,
        _: &mut AttributeMap<Key, Handle>,
        _: Option<Key>,
        _: &str,
    ) -> Result<bool, Error> {
        Ok(false)
    }
    fn supplier_value(
        &mut self,
        _: &mut AttributeMap<Key, Handle>,
        key: Option<Key>,
        kind: ValueKind,
    ) -> Result<f64, Error> {
        let key = key.ok_or(Error::NullAttribute)?;
        let base = self.base(&key).ok_or(Error::MissingAttribute)?;
        Ok(match kind {
            ValueKind::Base => base,
            ValueKind::Computed => self.catalog.attribute(key.id).definition.sanitize(base),
        })
    }
    fn supplier_modifier_value(
        &mut self,
        _: &mut AttributeMap<Key, Handle>,
        key: Option<Key>,
        _: &str,
    ) -> Result<f64, Error> {
        let key = key.ok_or(Error::NullAttribute)?;
        self.base(&key).ok_or(Error::MissingAttribute)?;
        Err(Error::MissingModifier)
    }
    fn instance_value(
        &mut self,
        _: &mut AttributeMap<Key, Handle>,
        h: Handle,
        kind: ValueKind,
    ) -> Result<f64, Error> {
        let value = &mut self.node_mut(&h)?.value;
        Ok(match kind {
            ValueKind::Computed => value.value(),
            ValueKind::Base => value.base_value(),
        })
    }
    fn instance_modifier(
        &mut self,
        _: &mut AttributeMap<Key, Handle>,
        h: Handle,
        id: &str,
    ) -> Result<Option<f64>, Error> {
        Ok(self.node(&h)?.value.modifier(id).map(|m| m.amount()))
    }
    fn set_instance_base(
        &mut self,
        map: &mut AttributeMap<Key, Handle>,
        h: Handle,
        value: f64,
    ) -> Result<(), Error> {
        self.edit(map, &h, |a, o| a.set_base_value(value, o))
    }
}
impl<G: IdentitySource> AttributeAccess<Key, Handle> for Data<G> {
    type Error = Error;
    fn create_instance(
        &mut self,
        _: &mut Definitions<Key>,
        map: &mut AttributeMap<Key, Handle>,
        key: Option<Key>,
    ) -> Result<Option<Handle>, Error> {
        self.create(map, key)
    }
    fn remove_modifier(
        &mut self,
        _: &mut Definitions<Key>,
        map: &mut AttributeMap<Key, Handle>,
        h: &Handle,
        id: &str,
    ) -> Result<(), Error> {
        self.edit(map, h, |a, o| a.remove(id, o)).map(|_| ())
    }
    fn add_permanent(
        &mut self,
        _: &mut Definitions<Key>,
        map: &mut AttributeMap<Key, Handle>,
        h: &Handle,
        modifier: Modifier,
    ) -> Result<(), Error> {
        self.edit(map, h, |a, o| a.add_permanent(modifier, o))
    }
}
/// Owns an entity's instances and pending queues. Holder hashes come from the
/// registry runtime; instance hashes come from the supplied identity allocator.
/// Supplying oracle hashes permits reproducible JVM-order differential checks.
pub struct LiveAttributes<G> {
    map: AttributeMap<Key, Handle>,
    data: Data<G>,
}
impl<G: IdentitySource> LiveAttributes<G> {
    pub fn new(
        catalog: Arc<Catalog>,
        entity: &str,
        holder_hashes: &[i32],
        next_identity: G,
    ) -> Result<Self, Error> {
        let bases = catalog.default_bases(entity).ok_or(Error::UnknownEntity)?;
        if holder_hashes.len() != bases.len() {
            return Err(Error::InvalidHolderHashes);
        }
        let keys = catalog
            .attributes()
            .map(|(id, _)| Key {
                catalog: catalog.clone(),
                id,
                hash: holder_hashes[id.index()],
            })
            .collect();
        Ok(Self {
            map: AttributeMap::default(),
            data: Data {
                catalog,
                bases,
                keys,
                owner: Arc::new(()),
                nodes: Vec::new(),
                next_identity,
            },
        })
    }
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.data.catalog
    }
    pub fn key(&self, name: &str) -> Option<Key> {
        self.data
            .catalog
            .id(name)
            .map(|id| self.data.keys[id.index()].clone())
    }
    pub fn keys(&self) -> &[Key] {
        &self.data.keys
    }
    pub fn lookup(&self, key: Option<&Key>) -> Option<Handle> {
        self.map.instances.get(key)
    }
    pub fn get_instance(&mut self, key: Option<Key>) -> Result<Option<Handle>, Error> {
        self.map
            .get_instance(key, |map, key| self.data.create(map, key))
    }
    pub fn has_attribute(&mut self, key: Option<Key>) -> Result<bool, Error> {
        self.map.has_attribute(key, &mut self.data)
    }
    pub fn has_modifier(&mut self, key: Option<Key>, id: &str) -> Result<bool, Error> {
        self.map.has_modifier(key, id, &mut self.data)
    }
    pub fn value(&mut self, key: Option<Key>, kind: ValueKind) -> Result<f64, Error> {
        self.map.value(key, kind, &mut self.data)
    }
    pub fn modifier_value(&mut self, key: Option<Key>, id: &str) -> Result<f64, Error> {
        self.map
            .modifier_value(key, id, &mut self.data)
            .map_err(|e| match e {
                ModifierValueError::World(e) => e,
                ModifierValueError::MissingInstanceModifier => Error::MissingInstanceModifier,
            })
    }
    pub fn reset_base(&mut self, key: Option<Key>) -> Result<bool, Error> {
        self.map.reset_base(key, &mut self.data)
    }
    pub fn instance(&self, handle: &Handle) -> Result<&Instance, Error> {
        self.data.node(handle).map(|n| &n.value)
    }
    pub fn attribute(&self, handle: &Handle) -> Result<&Key, Error> {
        self.data.node(handle).map(|n| &n.key)
    }
    pub fn instances(&self) -> impl Iterator<Item = (&Handle, &Key, &Instance)> {
        self.data
            .nodes
            .iter()
            .map(|n| (&n.handle, &n.key, &n.value))
    }
    pub fn entries(&self) -> Vec<(Option<Key>, Option<Handle>)> {
        self.map.instances.entries()
    }
    pub fn updates(&self) -> &ObjectOpenSet<Handle> {
        &self.map.updates
    }
    pub fn updates_mut(&mut self) -> &mut ObjectOpenSet<Handle> {
        &mut self.map.updates
    }
    pub fn sync(&self) -> &ObjectOpenSet<Handle> {
        &self.map.sync
    }
    pub fn sync_mut(&mut self) -> &mut ObjectOpenSet<Handle> {
        &mut self.map.sync
    }
    pub fn edit<R>(
        &mut self,
        handle: &Handle,
        edit: impl FnOnce(&mut Instance, &mut Dirty<'_>) -> Result<R, AttributeError<Infallible>>,
    ) -> Result<R, Error> {
        self.data.edit(&mut self.map, handle, edit)
    }
    pub fn add_effect_modifiers(
        &mut self,
        definitions: &mut Definitions<Key>,
        amplifier: i32,
    ) -> Result<(), effects::Error<Error>> {
        stored::add(definitions, &mut self.map, &mut self.data, amplifier)
    }
    pub fn remove_effect_modifiers(
        &mut self,
        definitions: &mut Definitions<Key>,
    ) -> Result<(), effects::Error<Error>> {
        stored::remove(definitions, &mut self.map, &mut self.data)
    }
}
