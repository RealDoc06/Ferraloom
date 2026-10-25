//! BannerBlockEntity's registry-bound pattern layers and implicit components.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use crate::block::containers::storage;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::Component,
    codec::{Holder, RegistryContext},
    component::{
        BannerLayer, ComponentTypeId, DataComponent, DataComponentMap, DyeColor, Persistent,
    },
};

/// Port of BannerBlockEntity. Base color is fixed by the constructor's block;
/// pattern-list decoding uses the caller's actual HolderLookup.Provider.
#[derive(Clone, Debug)]
pub struct BannerBlockEntity {
    pub parent: BlockEntityData,
    pub custom_name: Option<Component>,
    pub patterns: Vec<BannerLayer>,
    base_color: DyeColor,
}
impl BannerBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        let color = parent
            .block_state
            .block()
            .as_item()
            .data()
            .banner_color
            .expect("banner item has extracted native base color");
        Self {
            parent,
            custom_name: None,
            patterns: Vec::new(),
            base_color: DyeColor::from_nbt(&Tag::String(color.into())).expect("native dye color"),
        }
    }
    pub const fn base_color(&self) -> DyeColor {
        self.base_color
    }
    pub fn get_name(&self) -> Component {
        self.custom_name
            .clone()
            .unwrap_or_else(|| Component::translatable("block.minecraft.banner", Vec::new()))
    }
}

/// Port of TagValueInput#read(BannerPatternLayers.CODEC). The list codec keeps
/// successful layers as its partial result, including around an unknown holder.
pub(super) fn read_patterns(
    tag: &Tag,
    registries: &dyn RegistryContext,
) -> Option<Vec<BannerLayer>> {
    let list = tag.as_list()?;
    let registry = Identifier::parse("minecraft:banner_pattern").unwrap();
    Some(
        list.iter()
            .filter_map(|tag| {
                let layer = BannerLayer::from_nbt(tag).ok()?;
                if let Holder::Reference(id) = &layer.pattern.0 {
                    registries.entry_id(&registry, id)?;
                }
                Some(layer)
            })
            .collect(),
    )
}

pub(super) struct BannerBehavior;
pub(super) static BANNER_ENTITY: BannerBehavior = BannerBehavior;
impl BlockEntity for BannerBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        registries: &dyn RegistryContext,
    ) {
        let BlockEntityState::Banner(entity) = state else {
            unreachable!()
        };
        entity.custom_name = tag
            .get("CustomName")
            .and_then(|tag| Component::from_nbt(tag.clone()).ok());
        entity.patterns = tag
            .get("patterns")
            .and_then(|tag| read_patterns(tag, registries))
            .unwrap_or_default();
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Banner(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if !entity.patterns.is_empty() {
            tag.insert("patterns", entity.patterns.to_nbt()?);
        }
        if let Some(name) = &entity.custom_name {
            tag.insert("CustomName", name.to_nbt()?);
        }
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        state.save_without_metadata()
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Banner(entity) = state else {
            unreachable!()
        };
        entity.patterns = match map.get(ComponentTypeId::BANNER_PATTERNS) {
            Some(DataComponent::BannerPatterns(patterns)) => patterns.clone(),
            _ => Vec::new(),
        };
        entity.custom_name = match map.get(ComponentTypeId::CUSTOM_NAME) {
            Some(DataComponent::CustomName(name)) => Some((**name).clone()),
            _ => None,
        };
        &[
            ComponentTypeId::BANNER_PATTERNS,
            ComponentTypeId::CUSTOM_NAME,
        ]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Banner(entity) = state else {
            unreachable!()
        };
        storage::set(map, DataComponent::BannerPatterns(entity.patterns.clone()));
        crate::block::containers::storage::collect_custom_name(map, entity.custom_name.as_ref());
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("patterns");
        tag.remove("CustomName");
    }
}
