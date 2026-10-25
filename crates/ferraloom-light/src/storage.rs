use crate::DataLayer;
use crate::long_set::LongSet;
use ferraloom_util::core::{BlockPos, ChunkPos, SectionPos};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightLayer {
    Block,
    Sky,
}

/// Port of `LayerLightSectionStorage.SectionType`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SectionType {
    Empty,
    LightOnly,
    LightAndData,
}

/// Owned result for integration by a region at its ordered light-job poll point.
#[derive(Clone, Debug)]
pub struct LightSectionUpdate {
    pub layer: LightLayer,
    pub section: SectionPos,
    /// `None` removes a section. Clones share immutable arrays until a write.
    pub data: Option<Arc<DataLayer>>,
}

// New sections and their queued data alias the same Java DataLayer until the
// first reconcile. Represent that alias explicitly without mutable shared data.
#[derive(Debug)]
enum QueuedData {
    Pending(Arc<DataLayer>),
    Installed,
}

/// Port of `LayerLightSectionStorage` and both layer storage subclasses.
/// Updating and published maps share immutable layers; writes copy only once.
#[derive(Debug)]
pub(crate) struct Storage {
    pub layer: LightLayer,
    states: HashMap<i64, u8>,
    enabled: LongSet,
    pub updating: HashMap<i64, Arc<DataLayer>>,
    visible: HashMap<i64, Arc<DataLayer>>,
    queued: HashMap<i64, QueuedData>,
    retained: LongSet,
    remove: LongSet,
    changed: LongSet,
    affected: LongSet,
    pub inconsistent: bool,
    pub bottom: i32,
    tops: HashMap<i64, i32>,
    visible_bottom: i32,
    visible_tops: HashMap<i64, i32>,
}

impl Storage {
    pub fn new(layer: LightLayer) -> Self {
        Self {
            layer,
            states: HashMap::new(),
            enabled: LongSet::default(),
            updating: HashMap::new(),
            visible: HashMap::new(),
            queued: HashMap::new(),
            retained: LongSet::default(),
            remove: LongSet::default(),
            changed: LongSet::default(),
            affected: LongSet::default(),
            inconsistent: false,
            bottom: i32::MAX,
            tops: HashMap::new(),
            visible_bottom: i32::MAX,
            visible_tops: HashMap::new(),
        }
    }
    pub fn stores(&self, pos: SectionPos) -> bool {
        self.updating.contains_key(&pos.as_long())
    }
    pub fn enabled(&self, chunk: ChunkPos) -> bool {
        self.enabled.contains(&chunk.pack())
    }
    pub fn set_enabled(&mut self, chunk: ChunkPos, enabled: bool) {
        if enabled {
            self.enabled.insert(chunk.pack());
        } else {
            self.enabled.remove(&chunk.pack());
        }
    }
    pub fn retain(&mut self, chunk: ChunkPos, retain: bool) {
        if retain {
            self.retained.insert(chunk.pack());
        } else {
            self.retained.remove(&chunk.pack());
        }
    }
    pub fn section_type(&self, pos: SectionPos) -> SectionType {
        match self.states.get(&pos.as_long()).copied().unwrap_or(0) {
            0 => SectionType::Empty,
            state if state & 32 != 0 => SectionType::LightAndData,
            _ => SectionType::LightOnly,
        }
    }
    pub fn top(&self, chunk: ChunkPos) -> i32 {
        self.tops.get(&chunk.pack()).copied().unwrap_or(self.bottom)
    }
    pub fn stored(&self, pos: BlockPos) -> u8 {
        self.updating[&SectionPos::from_block(pos).as_long()].get(
            (pos.x & 15) as u8,
            (pos.y & 15) as u8,
            (pos.z & 15) as u8,
        )
    }
    pub fn set(&mut self, pos: BlockPos, level: u8) {
        let section = SectionPos::from_block(pos);
        self.write(section).unwrap().set(
            (pos.x & 15) as u8,
            (pos.y & 15) as u8,
            (pos.z & 15) as u8,
            level,
        );
        SectionPos::around_and_at_block_pos(pos, |s| {
            self.affected.insert(s);
        });
    }
    pub fn write(&mut self, pos: SectionPos) -> Option<&mut DataLayer> {
        let layer = self.updating.get_mut(&pos.as_long())?;
        self.changed.insert(pos.as_long());
        Some(Arc::make_mut(layer))
    }
    pub fn data(&self, pos: SectionPos) -> Option<&Arc<DataLayer>> {
        let key = pos.as_long();
        match self.queued.get(&key) {
            Some(QueuedData::Pending(data)) => Some(data),
            Some(QueuedData::Installed) => self.updating.get(&key),
            None => self.visible.get(&key),
        }
    }
    pub fn value(&self, pos: BlockPos) -> u8 {
        let mut section = SectionPos::from_block(pos);
        let mut y = (pos.y & 15) as u8;
        if self.layer == LightLayer::Sky {
            let top = self
                .visible_tops
                .get(&section.chunk().pack())
                .copied()
                .unwrap_or(self.visible_bottom);
            if top == self.visible_bottom || section.y >= top {
                return 15;
            }
            while !self.visible.contains_key(&section.as_long()) {
                section.y += 1;
                y = 0;
                if section.y >= top {
                    return 15;
                }
            }
        }
        self.visible
            .get(&section.as_long())
            .map_or(0, |d| d.get((pos.x & 15) as u8, y, (pos.z & 15) as u8))
    }
    pub fn queue(&mut self, pos: SectionPos, data: Option<DataLayer>) {
        if let Some(data) = data {
            self.queued
                .insert(pos.as_long(), QueuedData::Pending(Arc::new(data)));
            self.inconsistent = true;
        } else {
            self.queued.remove(&pos.as_long());
        }
    }
    pub fn update_status(&mut self, pos: SectionPos, empty: bool) {
        let old = self.states.get(&pos.as_long()).copied().unwrap_or(0);
        let new = if empty { old & !32 } else { old | 32 };
        if old == new {
            return;
        }
        self.put_state(pos, new);
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if x == 0 && y == 0 && z == 0 {
                        continue;
                    }
                    let neighbor = pos.offset(x, y, z);
                    let state = self.states.get(&neighbor.as_long()).copied().unwrap_or(0);
                    let count = i32::from(state & 31) + if empty { -1 } else { 1 };
                    assert!((0..=26).contains(&count));
                    self.put_state(neighbor, (state & 32) | count as u8);
                }
            }
        }
    }
    fn put_state(&mut self, pos: SectionPos, state: u8) {
        if state != 0 {
            if self.states.insert(pos.as_long(), state).is_none()
                && !self.remove.remove(&pos.as_long())
            {
                let data = self.create(pos);
                self.updating.insert(pos.as_long(), data);
                if let Some(queued) = self.queued.get_mut(&pos.as_long()) {
                    *queued = QueuedData::Installed;
                }
                self.changed.insert(pos.as_long());
                if self.layer == LightLayer::Sky {
                    self.bottom = self.bottom.min(pos.y);
                    let old = self.top(pos.chunk());
                    if old < pos.y + 1 {
                        self.tops.insert(pos.chunk().pack(), pos.y + 1);
                    }
                }
                for z in -1..=1 {
                    for x in -1..=1 {
                        for y in -1..=1 {
                            self.affected.insert(pos.offset(x, y, z).as_long());
                        }
                    }
                }
                self.inconsistent = true;
            }
        } else if self.states.remove(&pos.as_long()).is_some() {
            self.remove.insert(pos.as_long());
            self.inconsistent = true;
        }
    }
    fn create(&self, pos: SectionPos) -> Arc<DataLayer> {
        if let Some(QueuedData::Pending(data)) = self.queued.get(&pos.as_long()) {
            return data.clone();
        }
        if self.layer == LightLayer::Sky {
            let top = self.top(pos.chunk());
            if top != self.bottom && pos.y < top {
                for y in pos.y + 1..top {
                    if let Some(data) = self
                        .updating
                        .get(&SectionPos::new(pos.x, y, pos.z).as_long())
                    {
                        return Arc::new(data.repeat_first_layer());
                    }
                }
                unreachable!("top section must have light data");
            }
            if self.enabled(pos.chunk()) {
                return Arc::new(DataLayer::new(15));
            }
        }
        Arc::new(DataLayer::default())
    }
    pub fn reconcile(&mut self) {
        if !self.inconsistent {
            return;
        }
        self.inconsistent = false;
        for key in self.remove.iter() {
            let queued = self.queued.remove(key);
            let stored = self.updating.remove(key);
            let retained = match queued {
                Some(QueuedData::Pending(data)) => Some(data),
                Some(QueuedData::Installed) | None => stored,
            };
            if self.retained.contains(&SectionPos::of(*key).chunk().pack())
                && let Some(data) = retained
            {
                self.queued.insert(*key, QueuedData::Pending(data));
            }
        }
        for key in self.remove.drain() {
            let pos = SectionPos::of(key);
            if self.layer == LightLayer::Sky
                && self.tops.get(&pos.chunk().pack()) == Some(&(pos.y + 1))
            {
                let new_top = (self.bottom..=pos.y).rev().find(|y| {
                    self.updating
                        .contains_key(&SectionPos::new(pos.x, *y, pos.z).as_long())
                });
                if let Some(y) = new_top {
                    self.tops.insert(pos.chunk().pack(), y + 1);
                } else {
                    self.tops.remove(&pos.chunk().pack());
                }
            }
            self.changed.insert(key);
        }
        self.queued.retain(|key, data| {
            if let Some(stored) = self.updating.get_mut(key) {
                if let QueuedData::Pending(data) = data
                    && !Arc::ptr_eq(stored, data)
                {
                    *stored = data.clone();
                    self.changed.insert(*key);
                }
                false
            } else {
                true
            }
        });
    }
    pub fn publish(
        &mut self,
        updates: &mut Vec<LightSectionUpdate>,
        affected: &mut Vec<(LightLayer, SectionPos)>,
    ) {
        for key in self.changed.drain() {
            let data = self.updating.get(&key).cloned();
            if let Some(data) = &data {
                self.visible.insert(key, data.clone());
            } else {
                self.visible.remove(&key);
            }
            updates.push(LightSectionUpdate {
                layer: self.layer,
                section: SectionPos::of(key),
                data,
            });
        }
        self.visible_bottom = self.bottom;
        self.visible_tops.clone_from(&self.tops);
        affected.extend(
            self.affected
                .drain()
                .map(|s| (self.layer, SectionPos::of(s))),
        );
    }
}

#[cfg(test)]
mod published_read_tests {
    use super::*;

    fn pattern(sky: bool) -> Arc<DataLayer> {
        let mut data = DataLayer::default();
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    data.set(
                        x,
                        y,
                        z,
                        if sky {
                            x + 2 * y + 3 * z
                        } else {
                            2 * x + y + z
                        } & 15,
                    );
                }
            }
        }
        Arc::new(data)
    }
    fn storage(case: &str, layer: LightLayer) -> Storage {
        let mut storage = Storage::new(layer);
        if case == "empty" {
            return storage;
        }
        let section = SectionPos::new(9, 9, -1);
        storage.visible_bottom = -5;
        storage.visible_tops.insert(section.chunk().pack(), 12);
        storage.visible_tops.insert(ChunkPos::new(-1, -1).pack(), 1);
        match (case, layer) {
            ("initialized", LightLayer::Sky) => {
                storage
                    .visible
                    .insert(section.as_long(), Arc::new(DataLayer::default()));
            }
            ("initialized", LightLayer::Block) => {}
            ("pattern", LightLayer::Sky) => {
                for pos in [
                    section,
                    SectionPos::new(9, 11, -1),
                    SectionPos::new(-1, 0, -1),
                ] {
                    storage.visible.insert(pos.as_long(), pattern(true));
                }
            }
            ("pattern", LightLayer::Block) => {
                for pos in [section, SectionPos::new(-1, -2, -1)] {
                    storage.visible.insert(pos.as_long(), pattern(false));
                }
            }
            _ => panic!("unknown native fixture: {case}"),
        }
        // Poison both unpublished inputs: a layer read must ignore them.
        storage
            .updating
            .insert(section.as_long(), Arc::new(DataLayer::new(15)));
        storage.queue(section, Some(DataLayer::new(14)));
        storage
    }
    #[test]
    fn published_reads_match_native_top_holes_and_queued_layer_vectors() {
        let mut count = 0;
        for row in include_str!("../tests/fixtures/published_reads.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let pos = BlockPos::new(
                fields[1].parse().unwrap(),
                fields[2].parse().unwrap(),
                fields[3].parse().unwrap(),
            );
            let sky = storage(fields[0], LightLayer::Sky).value(pos);
            let block = storage(fields[0], LightLayer::Block).value(pos);
            let expected: Vec<u8> = fields[4..]
                .iter()
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(
                [sky, block, sky.max(block), sky.saturating_sub(5).max(block)].as_slice(),
                expected,
                "{row}"
            );
            count += 1;
        }
        assert_eq!(count, 36);
    }
}
