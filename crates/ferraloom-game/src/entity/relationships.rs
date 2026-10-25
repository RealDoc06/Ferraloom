//! Entity's passenger field replaces immutable lists, preserving captured views.
use super::EntityRef;
use crate::level::store::InsertOnlyMap;
use std::{
    cell::Cell,
    fmt,
    ops::{Deref, DerefMut},
};

/// Port of Entity's ImmutableList passenger field. Shared publication replaces
/// the current list; an earlier borrowed list keeps its original contents.
#[derive(Default)]
pub struct PassengerList {
    versions: InsertOnlyMap<u64, Vec<EntityRef>>,
    current: Cell<Option<u64>>,
}
impl PassengerList {
    pub(crate) fn replace_shared(&self, passengers: Vec<EntityRef>) {
        let version = self.current.get().map_or(0, |version| {
            version
                .checked_add(1)
                .expect("passenger list version exhausted")
        });
        self.versions
            .publish(version, passengers)
            .expect("new passenger list version");
        self.current.set(Some(version));
    }
}
impl Deref for PassengerList {
    type Target = Vec<EntityRef>;
    fn deref(&self) -> &Self::Target {
        static EMPTY: Vec<EntityRef> = Vec::new();
        self.current
            .get()
            .map_or(&EMPTY, |version| self.versions.get(&version).unwrap())
    }
}
impl DerefMut for PassengerList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if self.current.get().is_none() {
            self.replace_shared(Vec::new());
        }
        self.versions.get_mut(&self.current.get().unwrap()).unwrap()
    }
}
impl<'a> IntoIterator for &'a PassengerList {
    type Item = &'a EntityRef;
    type IntoIter = std::slice::Iter<'a, EntityRef>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl AsRef<[EntityRef]> for PassengerList {
    fn as_ref(&self) -> &[EntityRef] {
        self.as_slice()
    }
}
impl<T: AsRef<[EntityRef]>> PartialEq<T> for PassengerList {
    fn eq(&self, other: &T) -> bool {
        self.as_slice() == other.as_ref()
    }
}
impl Eq for PassengerList {}
impl fmt::Debug for PassengerList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.deref(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_lists_match_vanilla_immutable_passenger_replacements() {
        let passengers = PassengerList::default();
        let empty = passengers.as_slice();
        passengers.replace_shared(vec![EntityRef::fixture(crate::level::RegionId::FIXTURE, 2)]);
        let first = passengers.as_slice();
        passengers.replace_shared(vec![
            EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
            EntityRef::fixture(crate::level::RegionId::FIXTURE, 3),
        ]);
        let both = passengers.as_slice();
        passengers.replace_shared(vec![EntityRef::fixture(crate::level::RegionId::FIXTURE, 3)]);
        let ids = |list: &[EntityRef]| {
            list.iter()
                .map(|entity| entity.serial().to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let actual = format!(
            "passenger-lists\t{}\t{}\t{}\t{}",
            ids(empty),
            ids(first),
            ids(both),
            ids(&passengers)
        );
        let expected = include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .find(|line| line.starts_with("passenger-lists\t"))
            .unwrap();
        assert_eq!(actual, expected);
    }
}
