//! Canonical block-entity state, positional handles and stateless dispatch.
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap, DataComponentPatch};
use ferraloom_util::{BlockPos, ChunkPos};

mod components;
use super::containers::{
    chest::{BarrelBlockEntity, ChestBlockEntity, ShulkerBoxBlockEntity},
    crafter::CrafterBlockEntity,
    storage::{BaseContainerBlockEntity, RandomizableContainerBlockEntity},
};
pub mod registry;
pub mod ticking;
pub use registry::BlockEntityType;

/// A positional call handle. It never owns or takes canonical data out of a chunk.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockEntityRef {
    pub pos: BlockPos,
    pub generation: u64,
}

/// Port of BlockEntity's shared state. Mutable behavior belongs to the chunk.
#[derive(Clone, Debug)]
pub struct BlockEntityData {
    pub kind: BlockEntityType,
    pub pos: BlockPos,
    pub generation: u64,
    pub block_state: BlockState,
    pub removed: bool,
    pub has_level: bool,
    pub components: DataComponentMap,
}
impl BlockEntityData {
    pub fn new(
        kind: BlockEntityType,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<Self, LevelError> {
        if !registry::is_valid(kind, state) {
            return Err(LevelError::UnsupportedBlockEntity(state.block()));
        }
        Ok(Self {
            kind,
            pos,
            generation: 0,
            block_state: state,
            removed: false,
            has_level: false,
            components: DataComponentMap::default(),
        })
    }
    pub fn reference(&self) -> BlockEntityRef {
        BlockEntityRef {
            pos: self.pos,
            generation: self.generation,
        }
    }
    pub fn set_block_state(&mut self, state: BlockState) -> Result<(), LevelError> {
        if !registry::is_valid(self.kind, state) {
            return Err(LevelError::UnsupportedBlockEntity(state.block()));
        }
        self.block_state = state;
        Ok(())
    }
    pub fn set_removed(&mut self) {
        self.removed = true;
    }
    pub fn clear_removed(&mut self) {
        self.removed = false;
    }
}

/// Port of BlockEntity's virtual methods. Implementations are immutable dispatchers.
pub trait BlockEntity: Sync {
    fn load_additional(&self, _state: &mut BlockEntityState, _tag: &CompoundTag) {}
    fn save_additional(
        &self,
        _state: &BlockEntityState,
    ) -> ferraloom_protocol::Result<CompoundTag> {
        Ok(CompoundTag::new())
    }
    /// Return every component read, including absent keys, as vanilla's getter does.
    fn apply_implicit_components(
        &self,
        _state: &mut BlockEntityState,
        _components: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        &[]
    }
    fn collect_implicit_components(
        &self,
        _state: &BlockEntityState,
        _components: &mut DataComponentMap,
    ) {
    }
    fn get_update_tag(&self, _state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        Ok(CompoundTag::new())
    }
    fn sends_update_packet(&self) -> bool {
        false
    }
    fn has_server_ticker(&self) -> bool {
        false
    }
    fn tick(&self, _entity: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of BlockEntity#preRemoveSideEffects. Non-container base has no effects;
    /// registered container dispatchers override this before allowing removal.
    fn pre_remove_side_effects(
        &self,
        _entity: BlockEntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn trigger_event(&self, _state: &mut BlockEntityState, _event: i32, _value: i32) -> bool {
        false
    }
}
struct TheEndPortal;
impl BlockEntity for TheEndPortal {}
static THE_END_PORTAL: TheEndPortal = TheEndPortal;

/// Canonical concrete states. Extend this enum when porting a subclass, never Any.
#[derive(Clone, Debug)]
pub enum BlockEntityState {
    EndPortal(BlockEntityData),
    Chest(Box<ChestBlockEntity>),
    Barrel(Box<BarrelBlockEntity>),
    ShulkerBox(Box<ShulkerBoxBlockEntity>),
    Crafter(Box<CrafterBlockEntity>),
}
impl BlockEntityState {
    pub fn data(&self) -> &BlockEntityData {
        match self {
            Self::EndPortal(data) => data,
            Self::Chest(entity) => &entity.parent.parent.parent,
            Self::Barrel(entity) => &entity.parent.parent.parent,
            Self::ShulkerBox(entity) => &entity.parent.parent.parent,
            Self::Crafter(entity) => &entity.parent.parent.parent,
        }
    }
    pub fn data_mut(&mut self) -> &mut BlockEntityData {
        match self {
            Self::EndPortal(data) => data,
            Self::Chest(entity) => &mut entity.parent.parent.parent,
            Self::Barrel(entity) => &mut entity.parent.parent.parent,
            Self::ShulkerBox(entity) => &mut entity.parent.parent.parent,
            Self::Crafter(entity) => &mut entity.parent.parent.parent,
        }
    }
    pub fn behavior(&self) -> &'static dyn BlockEntity {
        match self {
            Self::EndPortal(_) => &THE_END_PORTAL,
            Self::Crafter(_) => &super::containers::crafter::CRAFTER_ENTITY,
            _ => &super::containers::chest::STORAGE_ENTITY,
        }
    }
    pub fn randomizable(&self) -> Option<&RandomizableContainerBlockEntity> {
        match self {
            Self::Chest(entity) => Some(&entity.parent),
            Self::Barrel(entity) => Some(&entity.parent),
            Self::ShulkerBox(entity) => Some(&entity.parent),
            Self::Crafter(entity) => Some(&entity.parent),
            _ => None,
        }
    }
    pub fn randomizable_mut(&mut self) -> Option<&mut RandomizableContainerBlockEntity> {
        match self {
            Self::Chest(entity) => Some(&mut entity.parent),
            Self::Barrel(entity) => Some(&mut entity.parent),
            Self::ShulkerBox(entity) => Some(&mut entity.parent),
            Self::Crafter(entity) => Some(&mut entity.parent),
            _ => None,
        }
    }
    pub fn container(&self) -> Option<&BaseContainerBlockEntity> {
        self.randomizable().map(|entity| &entity.parent)
    }
    pub fn container_mut(&mut self) -> Option<&mut BaseContainerBlockEntity> {
        self.randomizable_mut().map(|entity| &mut entity.parent)
    }
    /// Port of BlockEntity#loadWithComponents. Unknown/bad component values are reported by the codec and ignored.
    pub fn load_with_components(&mut self, tag: &CompoundTag) {
        self.behavior().load_additional(self, tag);
        self.data_mut().components = components::load(tag.get_compound("components"));
    }
    pub fn load_custom_only(&mut self, tag: &CompoundTag) {
        self.behavior().load_additional(self, tag);
    }
    pub fn save_custom_only(&self) -> ferraloom_protocol::Result<CompoundTag> {
        self.behavior().save_additional(self)
    }
    pub fn save_without_metadata(&self) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = self.save_custom_only()?;
        components::save_field(&mut tag, &self.data().components)?;
        Ok(tag)
    }
    pub fn save_with_id(&self) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = self.save_without_metadata()?;
        tag.insert("id", Tag::String(self.data().kind.name().into()));
        Ok(tag)
    }
    pub fn save_with_full_metadata(&self) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = self.save_with_id()?;
        let pos = self.data().pos;
        tag.insert("x", Tag::Int(pos.x));
        tag.insert("y", Tag::Int(pos.y));
        tag.insert("z", Tag::Int(pos.z));
        Ok(tag)
    }
    /// Only added patch entries not consumed by implicit fields are retained.
    pub fn apply_components(&mut self, prototype: &DataComponentMap, patch: &DataComponentPatch) {
        let mut full = prototype.clone();
        full.apply(patch);
        let consumed = self.behavior().apply_implicit_components(self, &full);
        self.data_mut().components = DataComponentMap {
            entries: patch
                .added
                .iter()
                .filter(|entry| {
                    !matches!(
                        entry.component_type(),
                        ComponentTypeId::BLOCK_ENTITY_DATA | ComponentTypeId::BLOCK_STATE
                    ) && !consumed.contains(&entry.component_type())
                })
                .cloned()
                .collect(),
        };
    }
    pub fn collect_components(&self) -> DataComponentMap {
        let mut result = self.data().components.clone();
        self.behavior()
            .collect_implicit_components(self, &mut result);
        result
    }
    pub fn get_update_tag(&self) -> ferraloom_protocol::Result<CompoundTag> {
        self.behavior().get_update_tag(self)
    }
    pub fn get_update_packet(
        &self,
    ) -> ferraloom_protocol::Result<Option<ferraloom_protocol::play::clientbound::BlockEntityData>>
    {
        if !self.behavior().sends_update_packet() {
            return Ok(None);
        }
        Ok(Some(
            ferraloom_protocol::play::clientbound::BlockEntityData {
                pos: self.data().pos,
                block_entity_type: ferraloom_protocol::play::BlockEntityTypeId(i32::from(
                    self.data().kind.protocol_id(),
                )),
                tag: ferraloom_protocol::play::TrustedCompoundTag(self.get_update_tag()?),
            },
        ))
    }
    pub fn chunk_packet_info(
        &self,
    ) -> ferraloom_protocol::Result<ferraloom_protocol::play::BlockEntityInfo> {
        let pos = self.data().pos;
        let tag = self.get_update_tag()?;
        Ok(ferraloom_protocol::play::BlockEntityInfo {
            packed_xz: (((pos.x & 15) << 4) | (pos.z & 15)) as u8,
            y: pos.y as i16,
            block_entity_type: ferraloom_protocol::play::BlockEntityTypeId(i32::from(
                self.data().kind.protocol_id(),
            )),
            tag: ferraloom_protocol::common::OptionalNbt(
                (!tag.is_empty()).then_some(Tag::Compound(tag)),
            ),
        })
    }
}
pub trait BlockEntityDataView: Sized {
    fn view(state: &BlockEntityState) -> Option<&Self>;
    fn view_mut(state: &mut BlockEntityState) -> Option<&mut Self>;
}
impl BlockEntityDataView for BlockEntityData {
    fn view(state: &BlockEntityState) -> Option<&Self> {
        Some(state.data())
    }
    fn view_mut(state: &mut BlockEntityState) -> Option<&mut Self> {
        Some(state.data_mut())
    }
}
/// Port of EntityBlock#newBlockEntity. Unported factories remain explicit errors.
pub fn create(pos: BlockPos, state: BlockState) -> Result<BlockEntityState, LevelError> {
    let kind = registry::for_block(state.block())
        .ok_or(LevelError::UnsupportedBlockEntity(state.block()))?;
    let data = BlockEntityData::new(kind, pos, state)?;
    match kind {
        BlockEntityType::END_PORTAL => Ok(BlockEntityState::EndPortal(data)),
        BlockEntityType::CHEST | BlockEntityType::TRAPPED_CHEST => Ok(BlockEntityState::Chest(
            Box::new(ChestBlockEntity::new(data)),
        )),
        BlockEntityType::BARREL => Ok(BlockEntityState::Barrel(Box::new(BarrelBlockEntity::new(
            data,
        )))),
        BlockEntityType::SHULKER_BOX => Ok(BlockEntityState::ShulkerBox(Box::new(
            ShulkerBoxBlockEntity::new(data),
        ))),
        BlockEntityType::CRAFTER => Ok(BlockEntityState::Crafter(Box::new(
            CrafterBlockEntity::new(data),
        ))),
        _ => Err(LevelError::UnsupportedBlockEntity(state.block())),
    }
}
/// Port of BlockEntity#loadStatic / LevelChunk#promotePendingBlockEntity.
/// An unsupported factory is an error so Warn callers retain the original NBT.
pub fn load_static(
    pos: BlockPos,
    state: BlockState,
    tag: &CompoundTag,
) -> Result<Option<BlockEntityState>, LevelError> {
    let id = tag
        .get_string("id")
        .map(|id| id.to_string_lossy())
        .unwrap_or_default();
    if id != "DUMMY" && BlockEntityType::from_name(&id) != registry::for_block(state.block()) {
        return Ok(None);
    }
    let mut entity = create(pos, state)?;
    if id != "DUMMY" {
        entity.load_with_components(tag);
    }
    Ok(Some(entity))
}
/// Port of BlockEntity#getPosFromTag: fix a misplaced entity's local X/Z.
pub fn get_pos_from_tag(base: ChunkPos, tag: &CompoundTag) -> BlockPos {
    let mut x = tag.get_int_or("x", 0);
    let y = tag.get_int_or("y", 0);
    let mut z = tag.get_int_or("z", 0);
    if x >> 4 != base.x || z >> 4 != base.z {
        x = base.x * 16 + (x & 15);
        z = base.z * 16 + (z & 15);
    }
    BlockPos::new(x, y, z)
}
/// Factory availability is separate from the complete vanilla validity registry.
pub fn supports(block: Block) -> bool {
    matches!(
        registry::for_block(block),
        Some(
            BlockEntityType::END_PORTAL
                | BlockEntityType::CHEST
                | BlockEntityType::TRAPPED_CHEST
                | BlockEntityType::BARREL
                | BlockEntityType::SHULKER_BOX
                | BlockEntityType::CRAFTER
        )
    )
}

#[cfg(test)]
mod tests;

macro_rules! projection {
    ($ty:ty,$variant:ident) => {
        impl BlockEntityDataView for $ty {
            fn view(state: &BlockEntityState) -> Option<&Self> {
                match state {
                    BlockEntityState::$variant(entity) => Some(entity),
                    _ => None,
                }
            }
            fn view_mut(state: &mut BlockEntityState) -> Option<&mut Self> {
                match state {
                    BlockEntityState::$variant(entity) => Some(entity),
                    _ => None,
                }
            }
        }
    };
}
projection!(ChestBlockEntity, Chest);
projection!(BarrelBlockEntity, Barrel);
projection!(ShulkerBoxBlockEntity, ShulkerBox);
projection!(CrafterBlockEntity, Crafter);
impl BlockEntityDataView for BaseContainerBlockEntity {
    fn view(state: &BlockEntityState) -> Option<&Self> {
        state.container()
    }
    fn view_mut(state: &mut BlockEntityState) -> Option<&mut Self> {
        state.container_mut()
    }
}
impl BlockEntityDataView for RandomizableContainerBlockEntity {
    fn view(state: &BlockEntityState) -> Option<&Self> {
        state.randomizable()
    }
    fn view_mut(state: &mut BlockEntityState) -> Option<&mut Self> {
        state.randomizable_mut()
    }
}
