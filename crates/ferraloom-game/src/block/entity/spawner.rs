//! BaseSpawner's persistent state and setEntityId. SpawnData references are
//! indices into one canonical arena, preserving the native weighted-list alias.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, EntityType};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{EquipmentSlot, JavaF32, Persistent},
};
use ferraloom_util::random::RandomSource;

#[derive(Clone, Debug)]
struct LightRange {
    min: i32,
    max: i32,
}
impl Default for LightRange {
    fn default() -> Self {
        Self { min: 0, max: 15 }
    }
}
impl LightRange {
    fn decode(tag: &Tag) -> Option<Self> {
        let (min, max) = if let Some(value) = tag.as_int() {
            (value, value)
        } else {
            let tag = tag.as_compound()?;
            (tag.get_int("min_inclusive")?, tag.get_int("max_inclusive")?)
        };
        (min <= max).then_some(Self { min, max })
    }
    fn save(&self) -> Tag {
        if self.min == self.max {
            return Tag::Int(self.min);
        }
        let mut tag = CompoundTag::new();
        tag.insert("min_inclusive", Tag::Int(self.min));
        tag.insert("max_inclusive", Tag::Int(self.max));
        Tag::Compound(tag)
    }
}
#[derive(Clone, Debug)]
struct SpawnRules {
    block: LightRange,
    sky: LightRange,
}
#[derive(Clone, Debug)]
struct SpawnEquipment {
    table: Identifier,
    chances: [Option<JavaF32>; 8],
}
const SLOTS: [EquipmentSlot; 8] = [
    EquipmentSlot::Mainhand,
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
    EquipmentSlot::Offhand,
    EquipmentSlot::Body,
    EquipmentSlot::Saddle,
];
impl SpawnEquipment {
    fn decode(tag: &CompoundTag) -> Option<Self> {
        let table =
            Identifier::parse(&tag.get_string("loot_table")?.to_string_checked().ok()?).ok()?;
        let mut chances = [None; 8];
        if let Some(value) = tag.get("slot_drop_chances") {
            if let Some(value) = value.as_float() {
                chances.fill(Some(JavaF32(value)));
            } else {
                for (slot, value) in value.as_compound()?.iter() {
                    let slot = EquipmentSlot::from_nbt(&Tag::String(slot.clone())).ok()?;
                    chances[slot as usize] = Some(JavaF32(value.as_float()?));
                }
            }
        }
        Some(Self { table, chances })
    }
    fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.insert("loot_table", Tag::String(self.table.as_str().into()));
        if self.chances.iter().any(Option::is_some) {
            let uniform = self.chances[0]
                .filter(|first| self.chances.iter().all(|value| *value == Some(*first)));
            let chances = if let Some(value) = uniform {
                Tag::Float(value.0)
            } else {
                let mut values = CompoundTag::new();
                for (slot, value) in SLOTS.into_iter().zip(self.chances) {
                    if let Some(value) = value {
                        let Tag::String(name) = slot.to_nbt().expect("native equipment slot codec")
                        else {
                            unreachable!()
                        };
                        values.insert(name, Tag::Float(value.0));
                    }
                }
                Tag::Compound(values)
            };
            tag.insert("slot_drop_chances", chances);
        }
        tag
    }
}
/// Port of SpawnData. Entity NBT stays native dynamic data; its optional spawn
/// rules and equipment retain the actual typed codec domains.
#[derive(Clone, Debug, Default)]
pub struct SpawnData {
    pub entity: CompoundTag,
    rules: Option<SpawnRules>,
    equipment: Option<SpawnEquipment>,
}
impl SpawnData {
    fn decode(tag: &CompoundTag) -> Option<Self> {
        let mut entity = tag.get_compound("entity")?.clone();
        let id = entity
            .get_string("id")
            .and_then(|id| id.to_string_checked().ok())
            .and_then(|id| Identifier::parse(&id).ok());
        entity.remove("id");
        if let Some(id) = id {
            entity.insert("id", Tag::String(id.as_str().into()));
        }
        // TagValueInput#read accepts SpawnData's partial value: an invalid
        // optional rules/equipment field becomes empty without losing entity.
        let rules = tag.get_compound("custom_spawn_rules").and_then(|value| {
            let rules = SpawnRules {
                block: value
                    .get("block_light_limit")
                    .and_then(LightRange::decode)
                    .unwrap_or_default(),
                sky: value
                    .get("sky_light_limit")
                    .and_then(LightRange::decode)
                    .unwrap_or_default(),
            };
            if rules.block.min < 0
                || rules.block.max > 15
                || rules.sky.min < 0
                || rules.sky.max > 15
            {
                return None;
            }
            Some(rules)
        });
        let equipment = tag
            .get_compound("equipment")
            .and_then(SpawnEquipment::decode);
        Some(Self {
            entity,
            rules,
            equipment,
        })
    }
    fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.insert("entity", Tag::Compound(self.entity.clone()));
        if let Some(rules) = &self.rules {
            let mut value = CompoundTag::new();
            for (name, range) in [
                ("block_light_limit", &rules.block),
                ("sky_light_limit", &rules.sky),
            ] {
                if range.min != 0 || range.max != 15 {
                    value.insert(name, range.save());
                }
            }
            tag.insert("custom_spawn_rules", Tag::Compound(value));
        }
        if let Some(equipment) = &self.equipment {
            tag.insert("equipment", Tag::Compound(equipment.save()));
        }
        tag
    }
}
#[derive(Clone, Debug)]
pub struct BaseSpawner {
    pub spawn_delay: i32,
    data: Vec<SpawnData>,
    next: Option<usize>,
    potentials: Vec<(usize, i32)>,
    pub min_spawn_delay: i32,
    pub max_spawn_delay: i32,
    pub spawn_count: i32,
    pub max_nearby_entities: i32,
    pub required_player_range: i32,
    pub spawn_range: i32,
}
impl Default for BaseSpawner {
    fn default() -> Self {
        Self {
            spawn_delay: 20,
            data: Vec::new(),
            next: None,
            potentials: Vec::new(),
            min_spawn_delay: 200,
            max_spawn_delay: 800,
            spawn_count: 4,
            max_nearby_entities: 6,
            required_player_range: 16,
            spawn_range: 4,
        }
    }
}
impl BaseSpawner {
    fn insert(&mut self, value: SpawnData) -> usize {
        let index = self.data.len();
        self.data.push(value);
        index
    }
    /// Port of getOrCreateNextSpawnData: even a single nonzero weight draws.
    pub fn get_or_create_next_spawn_data(
        &mut self,
        random: &mut dyn RandomSource,
    ) -> &mut SpawnData {
        let index = if let Some(index) = self.next {
            index
        } else {
            let total = self
                .potentials
                .iter()
                .map(|(_, weight)| *weight)
                .sum::<i32>();
            let index = if total == 0 {
                self.insert(SpawnData::default())
            } else {
                let mut selected = random.next_int_bound(total);
                self.potentials
                    .iter()
                    .find_map(|&(index, weight)| {
                        selected -= weight;
                        (selected < 0).then_some(index)
                    })
                    .expect("validated weighted spawn data")
            };
            self.next = Some(index);
            index
        };
        &mut self.data[index]
    }
    /// Port of BaseSpawner#setEntityId. Mutate the selected actual SpawnData,
    /// including the weighted entry when both references name that object.
    pub fn set_entity_id(&mut self, kind: EntityType, random: &mut dyn RandomSource) {
        self.get_or_create_next_spawn_data(random)
            .entity
            .insert("id", Tag::String(kind.name().into()));
    }
    pub fn load(&mut self, tag: &CompoundTag) {
        self.load_spawn_data(tag);
        self.load_remaining(tag);
    }
    /// BaseSpawner#load invokes the class's setNextSpawnData callback here,
    /// before replacing potentials and the remaining configuration fields.
    fn load_spawn_data(&mut self, tag: &CompoundTag) -> bool {
        self.spawn_delay = i32::from(tag.get_short_or("Delay", 20));
        if let Some(value) = tag.get_compound("SpawnData").and_then(SpawnData::decode) {
            self.next = Some(self.insert(value));
            true
        } else {
            false
        }
    }
    fn load_remaining(&mut self, tag: &CompoundTag) {
        let decoded = tag.get_list("SpawnPotentials").and_then(|list| {
            let mut total = 0i32;
            list.iter()
                .map(|value| {
                    let value = value.as_compound()?;
                    let weight = value.get_int("weight")?;
                    if weight < 0 {
                        return None;
                    }
                    total = total.checked_add(weight)?;
                    Some((SpawnData::decode(value.get_compound("data")?)?, weight))
                })
                .collect::<Option<Vec<_>>>()
        });
        self.potentials = if let Some(values) = decoded {
            values
                .into_iter()
                .map(|(value, weight)| (self.insert(value), weight))
                .collect()
        } else {
            let index = if let Some(index) = self.next {
                index
            } else {
                self.insert(SpawnData::default())
            };
            vec![(index, 1)]
        };
        self.min_spawn_delay = tag.get_int_or("MinSpawnDelay", 200);
        self.max_spawn_delay = tag.get_int_or("MaxSpawnDelay", 800);
        self.spawn_count = tag.get_int_or("SpawnCount", 4);
        self.max_nearby_entities = tag.get_int_or("MaxNearbyEntities", 6);
        self.required_player_range = tag.get_int_or("RequiredPlayerRange", 16);
        self.spawn_range = tag.get_int_or("SpawnRange", 4);
        // A load exposes only next/potential roots; reclaim earlier unreferenced
        // values without changing shared indices or invoking world callbacks.
        let mut remap = vec![None; self.data.len()];
        let mut old = 0;
        let mut new = 0;
        self.data.retain(|_| {
            let keep =
                self.next == Some(old) || self.potentials.iter().any(|(index, _)| *index == old);
            if keep {
                remap[old] = Some(new);
                new += 1;
            }
            old += 1;
            keep
        });
        self.next = self.next.map(|index| remap[index].unwrap());
        for (index, _) in &mut self.potentials {
            *index = remap[*index].unwrap();
        }
    }
    pub fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        for (name, value) in [
            ("Delay", self.spawn_delay),
            ("MinSpawnDelay", self.min_spawn_delay),
            ("MaxSpawnDelay", self.max_spawn_delay),
            ("SpawnCount", self.spawn_count),
            ("MaxNearbyEntities", self.max_nearby_entities),
            ("RequiredPlayerRange", self.required_player_range),
            ("SpawnRange", self.spawn_range),
        ] {
            tag.insert(name, Tag::Short(value as i16));
        }
        if let Some(index) = self.next {
            tag.insert("SpawnData", Tag::Compound(self.data[index].save()));
        }
        tag.insert(
            "SpawnPotentials",
            Tag::List(ListTag(
                self.potentials
                    .iter()
                    .map(|&(index, weight)| {
                        let mut value = CompoundTag::new();
                        value.insert("data", Tag::Compound(self.data[index].save()));
                        value.insert("weight", Tag::Int(weight));
                        Tag::Compound(value)
                    })
                    .collect(),
            )),
        );
        tag
    }
}
/// Port of SpawnerBlockEntity's embedded BaseSpawner. Server spawning remains
/// required Unsupported until the entity spawning hook is ported.
#[derive(Clone, Debug)]
pub struct SpawnerBlockEntity {
    pub parent: BlockEntityData,
    pub spawner: BaseSpawner,
}
impl SpawnerBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            spawner: BaseSpawner::default(),
        }
    }
}
pub(super) struct SpawnerBehavior;
pub(super) static SPAWNER_ENTITY: SpawnerBehavior = SpawnerBehavior;
impl BlockEntity for SpawnerBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, _handle: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(Block::SPAWNER))
    }
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::Spawner(entity) = state else {
            unreachable!()
        };
        entity.spawner.load(tag);
    }
    /// Port of SpawnerBlockEntity's setNextSpawnData override during load. The
    /// flag-260 notification reads the world's current state, while the rest of
    /// load continues on the original generation after that callback point.
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let notify = level
            .block_entity_mut::<SpawnerBlockEntity>(handle)?
            .spawner
            .load_spawn_data(tag);
        if notify && level.block_entity_state(handle)?.data().has_level {
            let state = level.get_block_state(handle.pos)?;
            level.send_block_updated(handle.pos, state, state);
        }
        level
            .block_entity_mut::<SpawnerBlockEntity>(handle)?
            .spawner
            .load_remaining(tag);
        Ok(())
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Spawner(entity) = state else {
            unreachable!()
        };
        Ok(entity.spawner.save())
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = self.save_additional(state)?;
        tag.remove("SpawnPotentials");
        Ok(tag)
    }
}

#[cfg(test)]
mod tests;
