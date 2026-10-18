//! StructureCheck's cheap existence checks. IO and generation-point probing are
//! caller-owned operations; this cache belongs to the level's exclusive lane.
use super::placement::StructurePlacement;
use crate::{Error, Identifier};
use ferraloom_nbt::CompoundTag;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureCheckResult {
    StartPresent,
    StartNotPresent,
    ChunkLoadNeeded,
}
/// A selective chunk scan after the storage layer's data-fixing step. Read or
/// data-fixing failures require a full chunk load; they are never cached absent.
pub enum StructureScan {
    Missing,
    Current(CompoundTag),
    Failed,
}
/// Port of StructureCheck. Immutable registries are shared; cache mutation is
/// explicitly borrowed, with no global locks or shared mutable generation RNG.
#[derive(Default)]
pub struct StructureCheck {
    loaded_chunks: BTreeMap<[i32; 2], BTreeMap<Identifier, i32>>,
    feature_checks: BTreeMap<Identifier, BTreeMap<[i32; 2], bool>>,
    chunks_without_starts_in_storage: BTreeSet<[i32; 2]>,
}
impl StructureCheck {
    #[allow(clippy::too_many_arguments)]
    pub fn check_start(
        &mut self,
        position: [i32; 2],
        structure: &Identifier,
        placement: &StructurePlacement,
        seed: i64,
        require_unreferenced: bool,
        registered: &BTreeSet<Identifier>,
        scan: &mut impl FnMut([i32; 2]) -> StructureScan,
        can_create: &mut impl FnMut([i32; 2], &Identifier) -> Result<bool, Error>,
    ) -> Result<StructureCheckResult, Error> {
        if let Some(known) = self.loaded_chunks.get(&position) {
            return Ok(check_info(known, structure, require_unreferenced));
        }
        if !self.chunks_without_starts_in_storage.contains(&position) {
            match scan(position) {
                StructureScan::Failed => return Ok(StructureCheckResult::ChunkLoadNeeded),
                StructureScan::Current(tag) => {
                    if let Some(known) = Self::load_structures(&tag, registered) {
                        let result = check_info(&known, structure, require_unreferenced);
                        self.store_full_results(position, known);
                        return Ok(result);
                    }
                }
                StructureScan::Missing => {}
            }
            if self.chunks_without_starts_in_storage.len() >= 131_072 {
                self.chunks_without_starts_in_storage.clear();
            }
            self.chunks_without_starts_in_storage.insert(position);
        }
        if placement
            .spreading()
            .is_some_and(|s| !s.apply_additional_chunk_restrictions(seed, position))
        {
            return Ok(StructureCheckResult::StartNotPresent);
        }
        let checks = self.feature_checks.entry(structure.clone()).or_default();
        let possible = if let Some(&value) = checks.get(&position) {
            value
        } else {
            let value = can_create(position, structure)?;
            checks.insert(position, value);
            value
        };
        Ok(if possible {
            StructureCheckResult::ChunkLoadNeeded
        } else {
            StructureCheckResult::StartNotPresent
        })
    }
    /// Only known registry keys and compound starts count. As in vanilla this
    /// cheap scan tests id != INVALID, without decoding Children or piece ids.
    pub fn load_structures(
        tag: &CompoundTag,
        registered: &BTreeSet<Identifier>,
    ) -> Option<BTreeMap<Identifier, i32>> {
        let starts = tag
            .get("structures")?
            .as_compound()?
            .get("starts")?
            .as_compound()?;
        Some(
            starts
                .iter()
                .filter_map(|(key, value)| {
                    let id: Identifier = key.to_string_lossy().parse().ok()?;
                    let tag = value.as_compound()?;
                    if !registered.contains(&id)
                        || tag
                            .get_string("id")
                            .is_some_and(|s| s.to_string_lossy() == "INVALID")
                    {
                        return None;
                    }
                    Some((id, tag.get_int("references").unwrap_or(0)))
                })
                .collect(),
        )
    }
    /// Port of onStructureLoad/storeFullResults. The map contains valid starts
    /// only, assembled by the owning chunk from StructureStart#isValid.
    pub fn on_structure_load(
        &mut self,
        position: [i32; 2],
        valid_starts: BTreeMap<Identifier, i32>,
    ) {
        self.store_full_results(position, valid_starts);
    }
    fn store_full_results(&mut self, position: [i32; 2], known: BTreeMap<Identifier, i32>) {
        self.loaded_chunks.insert(position, known);
        for checks in self.feature_checks.values_mut() {
            checks.remove(&position);
        }
        self.chunks_without_starts_in_storage.remove(&position);
    }
    pub fn increment_reference(&mut self, position: [i32; 2], structure: &Identifier) {
        let count = self
            .loaded_chunks
            .entry(position)
            .or_default()
            .entry(structure.clone())
            .or_default();
        *count = count.wrapping_add(1);
        self.chunks_without_starts_in_storage.remove(&position);
    }
}
fn check_info(
    known: &BTreeMap<Identifier, i32>,
    structure: &Identifier,
    require_unreferenced: bool,
) -> StructureCheckResult {
    let count = known.get(structure).copied().unwrap_or(-1);
    if count == -1 || (require_unreferenced && count != 0) {
        StructureCheckResult::StartNotPresent
    } else {
        StructureCheckResult::StartPresent
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
