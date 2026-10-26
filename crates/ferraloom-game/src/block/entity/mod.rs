//! Canonical block-entity state, positional handles and stateless dispatch.
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap, DataComponentPatch};
use ferraloom_util::{BlockPos, ChunkPos};

pub mod banner;
pub mod beacon;
pub mod beehive;
pub mod bell;
pub mod brewing_stand;
pub mod brushable;
pub mod campfire;
pub mod chiseled_bookshelf;
pub(crate) mod components;
pub mod conduit;
pub mod copper_golem_statue;
pub mod creaking_heart;
pub mod daylight_detector;
pub mod decorated_pot;
pub mod enchanting_table;
pub mod ender_chest;
pub mod jukebox;
pub mod lectern;
pub mod potent_sulfur;
pub mod sculk;
pub mod shelf;
pub mod spawner;
mod worldgen;
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
    /// Invoke the original receiver's class setRemoved under a call-frame root.
    pub fn set_removed_live(self, level: &mut Level) -> Result<(), LevelError> {
        level.with_block_entity_callback(self, |level| {
            let behavior = level.block_entity_state(self)?.behavior();
            behavior.set_removed_live(self, level)
        })
    }
    /// Live loadCustomOnly, used by TypedEntityData#loadInto. Class callbacks run
    /// on this generation; the existing extra component map stays untouched.
    pub fn load_custom_only(self, tag: &CompoundTag, level: &mut Level) -> Result<(), LevelError> {
        level.with_block_entity_callback(self, |level| {
            level.context().registry_context()?;
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
            level.context().registry_context()?;
            let behavior = level.block_entity_state(self)?.behavior();
            behavior.load_additional_live(self, tag, level)?;
            level.block_entity_mut::<BlockEntityData>(self)?.components = components::load(
                tag.get_compound("components"),
                level.context().registry_context()?,
            );
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
    fn load_additional(
        &self,
        _state: &mut BlockEntityState,
        _tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let context = level.context().clone();
        self.load_additional(
            level.block_entity_state_mut(handle)?,
            tag,
            context.registry_context()?,
        );
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
    /// Port of BlockEntity#setRemoved. Live overrides run on the retained
    /// generation while the world observes the native replacement/removal state.
    fn set_removed_live(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level
            .block_entity_mut::<BlockEntityData>(handle)?
            .set_removed();
        Ok(())
    }
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
    Jukebox(Box<jukebox::JukeboxBlockEntity>),
    CopperGolemStatue(Box<copper_golem_statue::CopperGolemStatueBlockEntity>),
    Shelf(Box<shelf::ShelfBlockEntity>),
    Conduit(Box<conduit::ConduitBlockEntity>),
    Beacon(Box<beacon::BeaconBlockEntity>),
    Campfire(Box<campfire::CampfireBlockEntity>),
    Bell(Box<bell::BellBlockEntity>),
    Lectern(Box<lectern::LecternBlockEntity>),
    DecoratedPot(Box<decorated_pot::DecoratedPotBlockEntity>),
    ChiseledBookShelf(Box<chiseled_bookshelf::ChiseledBookShelfBlockEntity>),
    CreakingHeart(Box<creaking_heart::CreakingHeartBlockEntity>),
    Banner(Box<banner::BannerBlockEntity>),
    BrewingStand(Box<brewing_stand::BrewingStandBlockEntity>),
    EnchantingTable(Box<enchanting_table::EnchantingTableBlockEntity>),
    EnderChest(Box<ender_chest::EnderChestBlockEntity>),
    SculkSensor(Box<sculk::SculkSensorBlockEntity>),
    CalibratedSculkSensor(Box<sculk::CalibratedSculkSensorBlockEntity>),
    SculkShrieker(Box<sculk::SculkShriekerBlockEntity>),
    SculkCatalyst(Box<sculk::SculkCatalystBlockEntity>),
    DaylightDetector(Box<daylight_detector::DaylightDetectorBlockEntity>),
    Brushable(Box<brushable::BrushableBlockEntity>),
    PotentSulfur(Box<potent_sulfur::PotentSulfurBlockEntity>),
    Command(Box<super::containers::command::CommandBlockEntity>),
    Structure(Box<super::containers::structure::StructureBlockEntity>),
    Skull(Box<super::containers::skull::SkullBlockEntity>),
    Spawner(Box<spawner::SpawnerBlockEntity>),
    Beehive(Box<beehive::BeehiveBlockEntity>),
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
            Self::Jukebox(entity) => &entity.parent,
            Self::CopperGolemStatue(entity) => &entity.parent,
            Self::Shelf(entity) => &entity.parent,
            Self::Conduit(entity) => &entity.parent,
            Self::Beacon(entity) => &entity.parent,
            Self::Campfire(entity) => &entity.parent,
            Self::Bell(entity) => &entity.parent,
            Self::Lectern(entity) => &entity.parent,
            Self::DecoratedPot(entity) => &entity.parent,
            Self::ChiseledBookShelf(entity) => &entity.parent,
            Self::CreakingHeart(entity) => &entity.parent,
            Self::Banner(entity) => &entity.parent,
            Self::BrewingStand(entity) => &entity.parent.parent,
            Self::EnchantingTable(entity) => &entity.parent,
            Self::EnderChest(entity) => &entity.parent,
            Self::SculkSensor(entity) => &entity.parent,
            Self::CalibratedSculkSensor(entity) => &entity.parent.parent,
            Self::SculkShrieker(entity) => &entity.parent,
            Self::SculkCatalyst(entity) => &entity.parent,
            Self::DaylightDetector(entity) => &entity.parent,
            Self::PotentSulfur(entity) => &entity.parent,
            Self::Command(entity) => &entity.parent,
            Self::Structure(entity) => &entity.parent,
            Self::Skull(entity) => &entity.parent,
            Self::Spawner(entity) => &entity.parent,
            Self::Beehive(entity) => &entity.parent,
            Self::Brushable(entity) => &entity.parent,
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
            Self::Jukebox(entity) => &mut entity.parent,
            Self::CopperGolemStatue(entity) => &mut entity.parent,
            Self::Shelf(entity) => &mut entity.parent,
            Self::Conduit(entity) => &mut entity.parent,
            Self::Beacon(entity) => &mut entity.parent,
            Self::Campfire(entity) => &mut entity.parent,
            Self::Bell(entity) => &mut entity.parent,
            Self::Lectern(entity) => &mut entity.parent,
            Self::DecoratedPot(entity) => &mut entity.parent,
            Self::ChiseledBookShelf(entity) => &mut entity.parent,
            Self::CreakingHeart(entity) => &mut entity.parent,
            Self::Banner(entity) => &mut entity.parent,
            Self::BrewingStand(entity) => &mut entity.parent.parent,
            Self::EnchantingTable(entity) => &mut entity.parent,
            Self::EnderChest(entity) => &mut entity.parent,
            Self::SculkSensor(entity) => &mut entity.parent,
            Self::CalibratedSculkSensor(entity) => &mut entity.parent.parent,
            Self::SculkShrieker(entity) => &mut entity.parent,
            Self::SculkCatalyst(entity) => &mut entity.parent,
            Self::DaylightDetector(entity) => &mut entity.parent,
            Self::PotentSulfur(entity) => &mut entity.parent,
            Self::Command(entity) => &mut entity.parent,
            Self::Structure(entity) => &mut entity.parent,
            Self::Skull(entity) => &mut entity.parent,
            Self::Spawner(entity) => &mut entity.parent,
            Self::Beehive(entity) => &mut entity.parent,
            Self::Brushable(entity) => &mut entity.parent,
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
            Self::Jukebox(_) => &jukebox::JUKEBOX_ENTITY,
            Self::CopperGolemStatue(_) => &copper_golem_statue::COPPER_GOLEM_STATUE_ENTITY,
            Self::Shelf(_) => &shelf::SHELF_ENTITY,
            Self::Conduit(_) => &conduit::CONDUIT_ENTITY,
            Self::Beacon(_) => &beacon::BEACON_ENTITY,
            Self::Campfire(_) => &campfire::CAMPFIRE_ENTITY,
            Self::Bell(_) => &bell::BELL_ENTITY,
            Self::Lectern(_) => &lectern::LECTERN_ENTITY,
            Self::DecoratedPot(_) => &decorated_pot::DECORATED_POT_ENTITY,
            Self::ChiseledBookShelf(_) => &chiseled_bookshelf::CHISELED_BOOKSHELF_ENTITY,
            Self::CreakingHeart(_) => &creaking_heart::CREAKING_HEART_ENTITY,
            Self::SculkSensor(_) | Self::CalibratedSculkSensor(_) | Self::SculkShrieker(_) => {
                &sculk::SCULK_ENTITY
            }
            Self::SculkCatalyst(_) => &sculk::CATALYST_ENTITY,
            Self::Banner(_) => &banner::BANNER_ENTITY,
            Self::BrewingStand(_) => &brewing_stand::BREWING_STAND_ENTITY,
            Self::EnchantingTable(_) => &enchanting_table::ENCHANTING_TABLE_ENTITY,
            Self::EnderChest(_) => &ender_chest::ENDER_CHEST_ENTITY,
            Self::DaylightDetector(_) => &daylight_detector::DAYLIGHT_DETECTOR_ENTITY,
            Self::PotentSulfur(_) => &potent_sulfur::POTENT_SULFUR_ENTITY,
            Self::Command(_) => &super::containers::command::COMMAND_ENTITY,
            Self::Structure(_) => &super::containers::structure::STRUCTURE_ENTITY,
            Self::Skull(_) => &super::containers::skull::SKULL_ENTITY,
            Self::Spawner(_) => &spawner::SPAWNER_ENTITY,
            Self::Beehive(_) => &beehive::BEEHIVE_ENTITY,
            Self::Brushable(_) => &brushable::BRUSHABLE_ENTITY,
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
    pub fn set_level(&mut self, game_time: i64, min_y: i32) {
        match self {
            Self::PotentSulfur(entity) => entity.set_level(game_time),
            Self::Beacon(entity) => entity.set_level(min_y),
            _ => self.data_mut().has_level = true,
        }
    }
    /// Port of EntityBlock#getTicker selection from cached state and the owning
    /// dimension's static fact. Types that do not use sky never require it.
    pub fn has_server_ticker(&self, has_skylight: Option<bool>) -> Result<bool, LevelError> {
        match self {
            Self::CreakingHeart(entity) => Ok(entity.has_server_ticker()),
            Self::Jukebox(entity) => Ok(entity.has_server_ticker()),
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
            Self::BrewingStand(entity) => Some(&entity.parent),
            _ => self.randomizable().map(|entity| &entity.parent),
        }
    }
    pub fn container_mut(&mut self) -> Option<&mut BaseContainerBlockEntity> {
        match self {
            Self::Furnace(entity) => Some(&mut entity.parent),
            Self::BrewingStand(entity) => Some(&mut entity.parent),
            _ => self.randomizable_mut().map(|entity| &mut entity.parent),
        }
    }
    /// Port of BlockEntity#loadWithComponents. Unknown/bad component values are reported by the codec and ignored.
    pub fn load_with_components(
        &mut self,
        tag: &CompoundTag,
        registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        self.behavior().load_additional(self, tag, registries);
        self.data_mut().components = components::load(tag.get_compound("components"), registries);
    }
    pub fn load_custom_only(
        &mut self,
        tag: &CompoundTag,
        registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        self.behavior().load_additional(self, tag, registries);
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
        BlockEntityType::JUKEBOX => Ok(BlockEntityState::Jukebox(Box::new(
            jukebox::JukeboxBlockEntity::new(data),
        ))),
        BlockEntityType::COPPER_GOLEM_STATUE => Ok(BlockEntityState::CopperGolemStatue(Box::new(
            copper_golem_statue::CopperGolemStatueBlockEntity::new(data),
        ))),
        BlockEntityType::SHELF => Ok(BlockEntityState::Shelf(Box::new(
            shelf::ShelfBlockEntity::new(data),
        ))),
        BlockEntityType::CONDUIT => Ok(BlockEntityState::Conduit(Box::new(
            conduit::ConduitBlockEntity::new(data),
        ))),
        BlockEntityType::BEACON => Ok(BlockEntityState::Beacon(Box::new(
            beacon::BeaconBlockEntity::new(data),
        ))),
        BlockEntityType::CAMPFIRE => Ok(BlockEntityState::Campfire(Box::new(
            campfire::CampfireBlockEntity::new(data),
        ))),
        BlockEntityType::BELL => Ok(BlockEntityState::Bell(Box::new(
            bell::BellBlockEntity::new(data),
        ))),
        BlockEntityType::LECTERN => Ok(BlockEntityState::Lectern(Box::new(
            lectern::LecternBlockEntity::new(data),
        ))),
        BlockEntityType::DECORATED_POT => Ok(BlockEntityState::DecoratedPot(Box::new(
            decorated_pot::DecoratedPotBlockEntity::new(data),
        ))),
        BlockEntityType::CHISELED_BOOKSHELF => Ok(BlockEntityState::ChiseledBookShelf(Box::new(
            chiseled_bookshelf::ChiseledBookShelfBlockEntity::new(data),
        ))),
        BlockEntityType::CREAKING_HEART => Ok(BlockEntityState::CreakingHeart(Box::new(
            creaking_heart::CreakingHeartBlockEntity::new(data),
        ))),
        BlockEntityType::BANNER => Ok(BlockEntityState::Banner(Box::new(
            banner::BannerBlockEntity::new(data),
        ))),
        BlockEntityType::BREWING_STAND => Ok(BlockEntityState::BrewingStand(Box::new(
            brewing_stand::BrewingStandBlockEntity::new(data),
        ))),
        BlockEntityType::ENCHANTING_TABLE => Ok(BlockEntityState::EnchantingTable(Box::new(
            enchanting_table::EnchantingTableBlockEntity::new(data),
        ))),
        BlockEntityType::ENDER_CHEST => Ok(BlockEntityState::EnderChest(Box::new(
            ender_chest::EnderChestBlockEntity::new(data),
        ))),
        BlockEntityType::SCULK_SENSOR => Ok(BlockEntityState::SculkSensor(Box::new(
            sculk::SculkSensorBlockEntity::new(data),
        ))),
        BlockEntityType::CALIBRATED_SCULK_SENSOR => Ok(BlockEntityState::CalibratedSculkSensor(
            Box::new(sculk::CalibratedSculkSensorBlockEntity::new(data)),
        )),
        BlockEntityType::SCULK_SHRIEKER => Ok(BlockEntityState::SculkShrieker(Box::new(
            sculk::SculkShriekerBlockEntity::new(data),
        ))),
        BlockEntityType::SCULK_CATALYST => Ok(BlockEntityState::SculkCatalyst(Box::new(
            sculk::SculkCatalystBlockEntity::new(data),
        ))),
        BlockEntityType::DAYLIGHT_DETECTOR => Ok(BlockEntityState::DaylightDetector(Box::new(
            daylight_detector::DaylightDetectorBlockEntity::new(data),
        ))),
        BlockEntityType::BRUSHABLE_BLOCK => Ok(BlockEntityState::Brushable(Box::new(
            brushable::BrushableBlockEntity::new(data),
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
        BlockEntityType::MOB_SPAWNER => Ok(BlockEntityState::Spawner(Box::new(
            spawner::SpawnerBlockEntity::new(data),
        ))),
        BlockEntityType::BEEHIVE => Ok(BlockEntityState::Beehive(Box::new(
            beehive::BeehiveBlockEntity::new(data),
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
    registries: &dyn ferraloom_protocol::codec::RegistryContext,
) -> Result<Option<BlockEntityState>, LevelError> {
    let id = tag
        .get_string("id")
        .map(|id| id.to_string_lossy())
        .unwrap_or_default();
    if id != "DUMMY"
        && (BlockEntityType::from_name(&id).is_none()
            || BlockEntityType::from_name(&id) != registry::for_block(state.block()))
    {
        return Ok(None);
    }
    let mut entity = create(pos, state)?;
    if id != "DUMMY" {
        entity.load_with_components(tag, registries);
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
            BlockEntityType::BEACON
                | BlockEntityType::CONDUIT
                | BlockEntityType::SHELF
                | BlockEntityType::COPPER_GOLEM_STATUE
                | BlockEntityType::JUKEBOX
                | BlockEntityType::CAMPFIRE
                | BlockEntityType::BELL
                | BlockEntityType::LECTERN
                | BlockEntityType::DECORATED_POT
                | BlockEntityType::CHISELED_BOOKSHELF
                | BlockEntityType::CREAKING_HEART
                | BlockEntityType::BANNER
                | BlockEntityType::BREWING_STAND
                | BlockEntityType::ENCHANTING_TABLE
                | BlockEntityType::ENDER_CHEST
                | BlockEntityType::SCULK_SENSOR
                | BlockEntityType::CALIBRATED_SCULK_SENSOR
                | BlockEntityType::SCULK_SHRIEKER
                | BlockEntityType::SCULK_CATALYST
                | BlockEntityType::DAYLIGHT_DETECTOR
                | BlockEntityType::POTENT_SULFUR
                | BlockEntityType::COMMAND_BLOCK
                | BlockEntityType::STRUCTURE_BLOCK
                | BlockEntityType::SKULL
                | BlockEntityType::BRUSHABLE_BLOCK
                | BlockEntityType::MOB_SPAWNER
                | BlockEntityType::BEEHIVE
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
mod lock_tests;
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
impl BlockEntityDataView for sculk::SculkSensorBlockEntity {
    fn view(state: &BlockEntityState) -> Option<&Self> {
        match state {
            BlockEntityState::SculkSensor(entity) => Some(entity),
            BlockEntityState::CalibratedSculkSensor(entity) => Some(&entity.parent),
            _ => None,
        }
    }
    fn view_mut(state: &mut BlockEntityState) -> Option<&mut Self> {
        match state {
            BlockEntityState::SculkSensor(entity) => Some(entity),
            BlockEntityState::CalibratedSculkSensor(entity) => Some(&mut entity.parent),
            _ => None,
        }
    }
}
projection!(
    sculk::CalibratedSculkSensorBlockEntity,
    CalibratedSculkSensor
);
projection!(sculk::SculkShriekerBlockEntity, SculkShrieker);
projection!(sculk::SculkCatalystBlockEntity, SculkCatalyst);
projection!(ChestBlockEntity, Chest);
projection!(spawner::SpawnerBlockEntity, Spawner);
projection!(beehive::BeehiveBlockEntity, Beehive);
projection!(brushable::BrushableBlockEntity, Brushable);
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
projection!(creaking_heart::CreakingHeartBlockEntity, CreakingHeart);
projection!(
    chiseled_bookshelf::ChiseledBookShelfBlockEntity,
    ChiseledBookShelf
);

projection!(banner::BannerBlockEntity, Banner);
projection!(brewing_stand::BrewingStandBlockEntity, BrewingStand);
projection!(
    enchanting_table::EnchantingTableBlockEntity,
    EnchantingTable
);
projection!(ender_chest::EnderChestBlockEntity, EnderChest);

#[cfg(test)]
mod factory_tests;

projection!(campfire::CampfireBlockEntity, Campfire);
projection!(bell::BellBlockEntity, Bell);
projection!(lectern::LecternBlockEntity, Lectern);
projection!(decorated_pot::DecoratedPotBlockEntity, DecoratedPot);

#[cfg(test)]
mod factory_two_tests;

projection!(beacon::BeaconBlockEntity, Beacon);

projection!(conduit::ConduitBlockEntity, Conduit);

projection!(shelf::ShelfBlockEntity, Shelf);

projection!(
    copper_golem_statue::CopperGolemStatueBlockEntity,
    CopperGolemStatue
);

projection!(jukebox::JukeboxBlockEntity, Jukebox);

#[cfg(test)]
mod factory_three_tests;
