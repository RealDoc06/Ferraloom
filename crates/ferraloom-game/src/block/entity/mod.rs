//! Canonical block-entity state, positional handles and stateless dispatch.
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap, DataComponentPatch};
use ferraloom_util::{BlockPos, ChunkPos};

mod components;
pub mod daylight_detector;
pub mod potent_sulfur;
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

impl BlockEntityRef {
    /// Live loadCustomOnly, used by TypedEntityData#loadInto. Class callbacks run
    /// on this generation; the existing extra component map stays untouched.
    pub fn load_custom_only(self, tag: &CompoundTag, level: &mut Level) -> Result<(), LevelError> {
        level.with_block_entity_callback(self, |level| {
            let behavior = level.block_entity_state(self)?.behavior();
            behavior.load_additional_live(self, tag, level)
        })
    }
    /// Live loadWithComponents: class callbacks complete on the retained receiver
    /// before the component map is replaced, matching BlockEntity's native order.
    pub fn load_with_components(
        self,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.with_block_entity_callback(self, |level| {
            let behavior = level.block_entity_state(self)?.behavior();
            behavior.load_additional_live(self, tag, level)?;
            level.block_entity_mut::<BlockEntityData>(self)?.components =
                components::load(tag.get_compound("components"));
            Ok(())
        })
    }
    /// Port of BlockEntity#setChanged, including comparator-neighbor updates.
    pub fn set_changed(self, level: &mut Level) -> Result<(), LevelError> {
        let data = level.block_entity::<BlockEntityData>(self)?;
        let (has_level, state) = (data.has_level, data.block_state);
        if has_level {
            level.mark_chunk_unsaved(ChunkPos::containing(self.pos))?;
            if !state.is_air() {
                level.update_neighbour_for_output_signal(self.pos, state.block())?;
            }
        }
        Ok(())
    }
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
    /// Port of EntityBlock#getListener for a GameEventListener.Provider receiver.
    fn game_event_listener(
        &self,
    ) -> Option<&'static dyn crate::level::game_event::GameEventListener> {
        None
    }
    fn load_additional(&self, _state: &mut BlockEntityState, _tag: &CompoundTag) {}
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        self.load_additional(level.block_entity_state_mut(handle)?, tag);
        Ok(())
    }
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
    fn remove_components_from_tag(&self, _tag: &mut CompoundTag) {}
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
    DaylightDetector(Box<daylight_detector::DaylightDetectorBlockEntity>),
    PotentSulfur(Box<potent_sulfur::PotentSulfurBlockEntity>),
    Command(Box<super::containers::command::CommandBlockEntity>),
    Structure(Box<super::containers::structure::StructureBlockEntity>),
    Skull(Box<super::containers::skull::SkullBlockEntity>),
    Sign(Box<super::containers::sign::SignBlockEntity>),
    Furnace(Box<super::containers::furnace::AbstractFurnaceBlockEntity>),
    Dispenser(Box<super::redstone::storage::DispenserBlockEntity>),
    Hopper(Box<super::redstone::storage::HopperBlockEntity>),
    MovingPiston(Box<super::redstone::moving_piston::PistonMovingBlockEntity>),
    EndPortal(BlockEntityData),
    Comparator(Box<super::redstone::comparator::ComparatorBlockEntity>),
    Chest(Box<ChestBlockEntity>),
    Barrel(Box<BarrelBlockEntity>),
    ShulkerBox(Box<ShulkerBoxBlockEntity>),
    Crafter(Box<CrafterBlockEntity>),
}
impl BlockEntityState {
    pub fn data(&self) -> &BlockEntityData {
        match self {
            Self::DaylightDetector(entity) => &entity.parent,
            Self::PotentSulfur(entity) => &entity.parent,
            Self::Command(entity) => &entity.parent,
            Self::Structure(entity) => &entity.parent,
            Self::Skull(entity) => &entity.parent,
            Self::Sign(entity) => &entity.parent,
            Self::Furnace(entity) => &entity.parent.parent,
            Self::Dispenser(entity) => &entity.parent.parent.parent,
            Self::Hopper(entity) => &entity.parent.parent.parent,
            Self::MovingPiston(entity) => &entity.parent,
            Self::EndPortal(data) => data,
            Self::Comparator(entity) => &entity.parent,
            Self::Chest(entity) => &entity.parent.parent.parent,
            Self::Barrel(entity) => &entity.parent.parent.parent,
            Self::ShulkerBox(entity) => &entity.parent.parent.parent,
            Self::Crafter(entity) => &entity.parent.parent.parent,
        }
    }
    pub fn data_mut(&mut self) -> &mut BlockEntityData {
        match self {
            Self::DaylightDetector(entity) => &mut entity.parent,
            Self::PotentSulfur(entity) => &mut entity.parent,
            Self::Command(entity) => &mut entity.parent,
            Self::Structure(entity) => &mut entity.parent,
            Self::Skull(entity) => &mut entity.parent,
            Self::Sign(entity) => &mut entity.parent,
            Self::Furnace(entity) => &mut entity.parent.parent,
            Self::Dispenser(entity) => &mut entity.parent.parent.parent,
            Self::Hopper(entity) => &mut entity.parent.parent.parent,
            Self::MovingPiston(entity) => &mut entity.parent,
            Self::EndPortal(data) => data,
            Self::Comparator(entity) => &mut entity.parent,
            Self::Chest(entity) => &mut entity.parent.parent.parent,
            Self::Barrel(entity) => &mut entity.parent.parent.parent,
            Self::ShulkerBox(entity) => &mut entity.parent.parent.parent,
            Self::Crafter(entity) => &mut entity.parent.parent.parent,
        }
    }
    pub fn behavior(&self) -> &'static dyn BlockEntity {
        match self {
            Self::DaylightDetector(_) => &daylight_detector::DAYLIGHT_DETECTOR_ENTITY,
            Self::PotentSulfur(_) => &potent_sulfur::POTENT_SULFUR_ENTITY,
            Self::Command(_) => &super::containers::command::COMMAND_ENTITY,
            Self::Structure(_) => &super::containers::structure::STRUCTURE_ENTITY,
            Self::Skull(_) => &super::containers::skull::SKULL_ENTITY,
            Self::Sign(_) => &super::containers::sign::SIGN_ENTITY,
            Self::Furnace(_) => &super::containers::furnace::FURNACE_ENTITY,
            Self::Dispenser(_) => &super::redstone::storage::DISPENSER_ENTITY,
            Self::Hopper(_) => &super::redstone::storage::HOPPER_ENTITY,
            Self::MovingPiston(_) => &super::redstone::moving_piston::MOVING_PISTON_ENTITY,
            Self::EndPortal(_) => &THE_END_PORTAL,
            Self::Comparator(_) => &super::redstone::comparator::COMPARATOR_ENTITY,
            Self::Crafter(_) => &super::containers::crafter::CRAFTER_ENTITY,
            _ => &super::containers::chest::STORAGE_ENTITY,
        }
    }
    /// Port of BlockEntity#setLevel and the supported subclass initialization.
    /// These assignments use the current clock without invoking world callbacks.
    pub fn set_level(&mut self, game_time: i64) {
        match self {
            Self::PotentSulfur(entity) => entity.set_level(game_time),
            _ => self.data_mut().has_level = true,
        }
    }
    /// Port of EntityBlock#getTicker selection from cached state and the owning
    /// dimension's static fact. Types that do not use sky never require it.
    pub fn has_server_ticker(&self, has_skylight: Option<bool>) -> Result<bool, LevelError> {
        match self {
            Self::DaylightDetector(_) => has_skylight.ok_or(LevelError::UnsupportedLightQuery),
            Self::PotentSulfur(entity) => Ok(entity.has_server_ticker()),
            _ => Ok(self.behavior().has_server_ticker()),
        }
    }
    pub fn randomizable(&self) -> Option<&RandomizableContainerBlockEntity> {
        match self {
            Self::Dispenser(entity) => Some(&entity.parent),
            Self::Hopper(entity) => Some(&entity.parent),
            Self::Chest(entity) => Some(&entity.parent),
            Self::Barrel(entity) => Some(&entity.parent),
            Self::ShulkerBox(entity) => Some(&entity.parent),
            Self::Crafter(entity) => Some(&entity.parent),
            _ => None,
        }
    }
    pub fn randomizable_mut(&mut self) -> Option<&mut RandomizableContainerBlockEntity> {
        match self {
            Self::Dispenser(entity) => Some(&mut entity.parent),
            Self::Hopper(entity) => Some(&mut entity.parent),
            Self::Chest(entity) => Some(&mut entity.parent),
            Self::Barrel(entity) => Some(&mut entity.parent),
            Self::ShulkerBox(entity) => Some(&mut entity.parent),
            Self::Crafter(entity) => Some(&mut entity.parent),
            _ => None,
        }
    }
    pub fn container(&self) -> Option<&BaseContainerBlockEntity> {
        match self {
            Self::Furnace(entity) => Some(&entity.parent),
            _ => self.randomizable().map(|entity| &entity.parent),
        }
    }
    pub fn container_mut(&mut self) -> Option<&mut BaseContainerBlockEntity> {
        match self {
            Self::Furnace(entity) => Some(&mut entity.parent),
            _ => self.randomizable_mut().map(|entity| &mut entity.parent),
        }
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
    pub fn apply_components_from_item_stack(
        &mut self,
        stack: &ferraloom_protocol::component::ItemStack,
    ) -> ferraloom_protocol::Result<()> {
        if let Some(stack) = &stack.0 {
            self.apply_components(
                DataComponentMap::item_defaults(stack.item)?,
                &stack.components,
            );
        } else {
            self.apply_components(&DataComponentMap::default(), &DataComponentPatch::default());
        }
        Ok(())
    }
    pub fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        self.behavior().remove_components_from_tag(tag);
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
        BlockEntityType::DAYLIGHT_DETECTOR => Ok(BlockEntityState::DaylightDetector(Box::new(
            daylight_detector::DaylightDetectorBlockEntity::new(data),
        ))),
        BlockEntityType::POTENT_SULFUR => Ok(BlockEntityState::PotentSulfur(Box::new(
            potent_sulfur::PotentSulfurBlockEntity::new(data),
        ))),
        BlockEntityType::COMMAND_BLOCK => Ok(BlockEntityState::Command(Box::new(
            super::containers::command::CommandBlockEntity::new(data),
        ))),
        BlockEntityType::STRUCTURE_BLOCK => Ok(BlockEntityState::Structure(Box::new(
            super::containers::structure::StructureBlockEntity::new(data),
        ))),
        BlockEntityType::SKULL => Ok(BlockEntityState::Skull(Box::new(
            super::containers::skull::SkullBlockEntity::new(data),
        ))),
        BlockEntityType::END_PORTAL => Ok(BlockEntityState::EndPortal(data)),
        BlockEntityType::PISTON => Ok(BlockEntityState::MovingPiston(Box::new(
            super::redstone::moving_piston::PistonMovingBlockEntity::new(data),
        ))),
        BlockEntityType::DISPENSER | BlockEntityType::DROPPER => Ok(BlockEntityState::Dispenser(
            Box::new(super::redstone::storage::DispenserBlockEntity::new(data)),
        )),
        BlockEntityType::HOPPER => Ok(BlockEntityState::Hopper(Box::new(
            super::redstone::storage::HopperBlockEntity::new(data),
        ))),
        BlockEntityType::SIGN | BlockEntityType::HANGING_SIGN => Ok(BlockEntityState::Sign(
            Box::new(super::containers::sign::SignBlockEntity::new(data)),
        )),
        BlockEntityType::FURNACE | BlockEntityType::SMOKER | BlockEntityType::BLAST_FURNACE => {
            Ok(BlockEntityState::Furnace(Box::new(
                super::containers::furnace::AbstractFurnaceBlockEntity::new(data),
            )))
        }
        BlockEntityType::COMPARATOR => Ok(BlockEntityState::Comparator(Box::new(
            super::redstone::comparator::ComparatorBlockEntity::new(data),
        ))),
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
            BlockEntityType::DAYLIGHT_DETECTOR
                | BlockEntityType::POTENT_SULFUR
                | BlockEntityType::COMMAND_BLOCK
                | BlockEntityType::STRUCTURE_BLOCK
                | BlockEntityType::SKULL
                | BlockEntityType::END_PORTAL
                | BlockEntityType::PISTON
                | BlockEntityType::DISPENSER
                | BlockEntityType::DROPPER
                | BlockEntityType::HOPPER
                | BlockEntityType::SIGN
                | BlockEntityType::HANGING_SIGN
                | BlockEntityType::FURNACE
                | BlockEntityType::SMOKER
                | BlockEntityType::BLAST_FURNACE
                | BlockEntityType::COMPARATOR
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
projection!(super::containers::command::CommandBlockEntity, Command);
projection!(
    super::containers::structure::StructureBlockEntity,
    Structure
);
projection!(super::containers::skull::SkullBlockEntity, Skull);
projection!(
    daylight_detector::DaylightDetectorBlockEntity,
    DaylightDetector
);
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

projection!(
    super::redstone::comparator::ComparatorBlockEntity,
    Comparator
);

projection!(super::containers::sign::SignBlockEntity, Sign);
projection!(
    super::containers::furnace::AbstractFurnaceBlockEntity,
    Furnace
);

/// Port of EntityBlock#newBlockEntity, whose nullable result is distinct from an
/// unported factory. MovingPistonBlock creates no entity until the piston installs
/// its explicit moved-state receiver after setBlock returns.
pub fn new_block_entity(
    pos: BlockPos,
    state: BlockState,
) -> Result<Option<BlockEntityState>, LevelError> {
    if state.block() == Block::MOVING_PISTON {
        return Ok(None);
    }
    let mut entity = create(pos, state)?;
    // CrafterBlock#newBlockEntity initializes this scalar after the typed ctor;
    // BlockEntityType#create/loadStatic retain the constructor's zero default.
    if let BlockEntityState::Crafter(crafter) = &mut entity {
        crafter.triggered =
            i32::from(state.get(ferraloom_data::properties::TRIGGERED) == Some(true));
    }
    Ok(Some(entity))
}
pub fn supports_creation(block: Block) -> bool {
    block == Block::MOVING_PISTON || supports(block)
}

projection!(super::redstone::storage::DispenserBlockEntity, Dispenser);
projection!(super::redstone::storage::HopperBlockEntity, Hopper);

projection!(
    super::redstone::moving_piston::PistonMovingBlockEntity,
    MovingPiston
);

#[cfg(test)]
mod placement_tests;

projection!(potent_sulfur::PotentSulfurBlockEntity, PotentSulfur);
