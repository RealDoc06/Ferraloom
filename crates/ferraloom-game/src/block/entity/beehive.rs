//! Beehive storage and worldgen occupants. Bee creation/release remains a
//! required entity hook; generation stores only native occupant data.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, EntityType};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{
    codec::VarInt,
    component::{
        BeeOccupant, ComponentTypeId, CustomData, DataComponent, DataComponentMap, Persistent,
        TypedEntityData,
    },
};
use ferraloom_util::{BlockPos, random::RandomSource};

/// Port of BeehiveBlockEntity's stored BeeData/Occupant values. No live Bee is
/// constructed when a tree decorator fills a nest.
#[derive(Clone, Debug)]
pub struct BeehiveBlockEntity {
    pub parent: BlockEntityData,
    pub stored: Vec<BeeOccupant>,
    pub saved_flower_pos: Option<BlockPos>,
}
impl BeehiveBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            stored: Vec::new(),
            saved_flower_pos: None,
        }
    }
    /// Port of BeehiveDecorator's getBlockEntity(..., BEEHIVE).ifPresent body.
    pub fn populate_worldgen(&mut self, random: &mut dyn RandomSource) {
        let count = 2 + random.next_int_bound(2);
        for _ in 0..count {
            self.stored.push(BeeOccupant {
                entity_data: TypedEntityData {
                    entity_type: ferraloom_protocol::component::EntityType(
                        ferraloom_protocol::Identifier::parse(EntityType::BEE.name()).unwrap(),
                    ),
                    tag: CustomData(Tag::Compound(CompoundTag::new())),
                },
                ticks_in_hive: VarInt(random.next_int_bound(599)),
                min_ticks_in_hive: VarInt(600),
            });
        }
    }
}
pub(super) struct BeehiveBehavior;
pub(super) static BEEHIVE_ENTITY: BeehiveBehavior = BeehiveBehavior;
impl BlockEntity for BeehiveBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, _receiver: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(Block::BEEHIVE))
    }
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::Beehive(entity) = state else {
            unreachable!()
        };
        entity.stored = tag
            .get("bees")
            .and_then(|tag| Vec::<BeeOccupant>::from_nbt(tag).ok())
            .filter(|bees| {
                bees.iter().all(|bee| {
                    EntityType::from_name(bee.entity_data.entity_type.0.as_str()).is_some()
                })
            })
            .unwrap_or_default();
        entity.saved_flower_pos = tag.get("flower_pos").and_then(|value| {
            let coordinates: Vec<_> = match value {
                Tag::IntArray(values) => values.clone(),
                Tag::List(values) => values.iter().map(Tag::as_int).collect::<Option<Vec<_>>>()?,
                _ => return None,
            };
            let [x, y, z] = coordinates.as_slice() else {
                return None;
            };
            Some(BlockPos::new(*x, *y, *z))
        });
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Beehive(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        tag.insert(
            "bees",
            Tag::List(ListTag(
                entity
                    .stored
                    .iter()
                    .map(Persistent::to_nbt)
                    .collect::<ferraloom_protocol::Result<_>>()?,
            )),
        );
        if let Some(pos) = entity.saved_flower_pos {
            tag.insert("flower_pos", Tag::IntArray(vec![pos.x, pos.y, pos.z]));
        }
        Ok(tag)
    }
    fn apply_implicit_components(&self, state: &mut BlockEntityState, map: &DataComponentMap) -> &'static [ComponentTypeId] {
        let BlockEntityState::Beehive(entity) = state else {
            unreachable!()
        };
        entity.stored = match map.get(ComponentTypeId::BEES) {
            Some(DataComponent::Bees(bees)) => bees.clone(),
            _ => Vec::new(),
        };
        &[ComponentTypeId::BEES]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Beehive(entity) = state else {
            unreachable!()
        };
        crate::block::containers::storage::set(map, DataComponent::Bees(entity.stored.clone()));
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("bees");
    }
}
