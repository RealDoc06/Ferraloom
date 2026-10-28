use super::{Occupancy, PoiError, PoiRecord, PoiTypeId};
use ferraloom_util::{
    BlockPos, SectionPos,
    collections::{HashSet, Short2ObjectOpenHashMap},
};
use std::collections::BTreeMap;

/// Port of PoiSection's position map and per-type, position-hashed record sets.
#[derive(Clone, Debug)]
pub struct PoiSection {
    pub valid: bool,
    // Native Short2ObjectOpenHashMap; its value traversal determines pack order.
    records: Short2ObjectOpenHashMap<PoiRecord>,
    // Native identity-keyed HashMap order is non-parity; use registry-id order.
    // Each value retains native position-hashed HashSet traversal for queries.
    by_type: BTreeMap<PoiTypeId, HashSet<BlockPos>>,
    // Native add logs mismatched duplicates and retains the earlier record in
    // its old type set. These are distinct objects, not mirrored primary state.
    // Lookup-only ownership of those retained objects. The section-local short
    // is the primary record key; this map is never traversed to emit records.
    replaced: BTreeMap<(PoiTypeId, i16), PoiRecord>,
}
impl Default for PoiSection {
    fn default() -> Self {
        Self {
            valid: true,
            records: Default::default(),
            by_type: Default::default(),
            replaced: Default::default(),
        }
    }
}
impl PoiSection {
    fn key(pos: BlockPos) -> i16 {
        SectionPos::section_relative_pos(pos) as i16
    }
    pub fn get(&self, pos: BlockPos) -> Option<&PoiRecord> {
        self.records.get(&Self::key(pos))
    }
    pub fn get_mut(&mut self, pos: BlockPos) -> Option<&mut PoiRecord> {
        self.records.get_mut(&Self::key(pos))
    }
    pub fn add(&mut self, record: PoiRecord) -> bool {
        if self
            .get(record.pos)
            .is_some_and(|old| old.poi_type == record.poi_type)
        {
            return false;
        }
        if let Some(old) = self.get(record.pos).cloned() {
            self.replaced
                .entry((old.poi_type, Self::key(old.pos)))
                .or_insert(old);
        }
        self.by_type
            .entry(record.poi_type)
            .or_default()
            .insert(record.pos);
        self.records.insert(Self::key(record.pos), record);
        true
    }
    pub fn remove(&mut self, pos: BlockPos) -> bool {
        if let Some(record) = self.records.remove(&Self::key(pos)) {
            self.replaced.remove(&(record.poi_type, Self::key(pos)));
            self.by_type
                .get_mut(&record.poi_type)
                .expect("indexed record")
                .remove(&pos);
            true
        } else {
            false
        }
    }
    pub fn records(
        &self,
        mut predicate: impl FnMut(PoiTypeId) -> bool,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        // Holder identity-hash traversal is non-parity; use registry-id order.
        // Position-hashed sets retain native value-defined traversal order.
        let mut result = Vec::new();
        for (kind, positions) in &self.by_type {
            if predicate(*kind) {
                for pos in positions.iter() {
                    let record = self
                        .replaced
                        .get(&(*kind, Self::key(*pos)))
                        .or_else(|| self.get(*pos))
                        .expect("indexed POI");
                    if occupancy.test(record) {
                        result.push(record.clone());
                    }
                }
            }
        }
        result
    }
    pub fn record_for_type_mut(
        &mut self,
        kind: PoiTypeId,
        pos: BlockPos,
    ) -> Option<&mut PoiRecord> {
        if self.replaced.contains_key(&(kind, Self::key(pos))) {
            self.replaced.get_mut(&(kind, Self::key(pos)))
        } else {
            self.get_mut(pos).filter(|record| record.poi_type == kind)
        }
    }

    pub fn pack(&self) -> ferraloom_storage::PoiSection {
        use ferraloom_storage::{IntArray, PoiRecord as PackedRecord, PoiSection as PackedSection};
        PackedSection {
            valid: Some(self.valid),
            records: Some(
                self.records
                    .values()
                    .map(|record| PackedRecord {
                        position: Some(IntArray(vec![record.pos.x, record.pos.y, record.pos.z])),
                        poi_type: Some(record.poi_type.name().into()),
                        free_tickets: Some(record.free_tickets),
                        ..Default::default()
                    })
                    .collect(),
            ),
            ..Default::default()
        }
    }
    pub fn unpack(packed: ferraloom_storage::PoiSection) -> Result<Self, PoiError> {
        let mut section = Self {
            valid: packed.valid.unwrap_or(false),
            ..Default::default()
        };
        let records = packed
            .records
            .ok_or_else(|| PoiError::InvalidData("POI section requires Records".into()))?;
        for record in records {
            let pos = record
                .position
                .ok_or_else(|| PoiError::InvalidData("POI record requires pos".into()))?;
            let [x, y, z] = pos.0.as_slice() else {
                return Err(PoiError::InvalidData(
                    "POI pos must have three coordinates".into(),
                ));
            };
            let name = record
                .poi_type
                .ok_or_else(|| PoiError::InvalidData("POI record requires type".into()))?;
            let name = name
                .to_string_checked()
                .map_err(|_| PoiError::InvalidData("invalid POI identifier encoding".into()))?;
            let poi_type = PoiTypeId::from_name(&name)
                .ok_or_else(|| PoiError::InvalidData(format!("unknown POI type {name}")))?;
            section.add(PoiRecord {
                pos: BlockPos::new(*x, *y, *z),
                poi_type,
                free_tickets: record.free_tickets.unwrap_or(0),
            });
        }
        Ok(section)
    }
    /// Port of PoiSection#refresh. Invalid sections retain old records/tickets
    /// by position, including native behavior when the observed type changed.
    pub fn refresh(&mut self, points: impl IntoIterator<Item = (BlockPos, PoiTypeId)>) -> bool {
        if self.valid {
            return false;
        }
        let mut old = std::mem::take(&mut self.records);
        self.by_type.clear();
        self.replaced.clear();
        for (pos, kind) in points {
            let record = old
                .get_or_insert_with(Self::key(pos), || PoiRecord::new(pos, kind))
                .clone();
            self.add(record);
        }
        self.valid = true;
        true
    }
}
