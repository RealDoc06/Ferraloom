//! Decorated-pot persistence and its canonical single item/loot-table state.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use crate::{block::containers::storage, inventory::StackGenerations};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{
        ComponentTypeId, DataComponent, DataComponentMap, ItemStack, ItemStackTemplate, Persistent,
        PotDecorations,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WobbleStyle {
    Positive,
    Negative,
}
/// Port of DecoratedPotBlockEntity; it does not extend BaseContainerBlockEntity.
#[derive(Clone, Debug)]
pub struct DecoratedPotBlockEntity {
    pub parent: BlockEntityData,
    pub decorations: PotDecorations,
    pub item: ItemStack,
    pub loot_table: Option<Identifier>,
    pub loot_table_seed: i64,
    pub wobble_started_at_tick: i64,
    pub last_wobble_style: Option<WobbleStyle>,
    stack_generations: StackGenerations,
}
impl DecoratedPotBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            decorations: empty_decorations(),
            item: ItemStack::default(),
            loot_table: None,
            loot_table_seed: 0,
            wobble_started_at_tick: 0,
            last_wobble_style: None,
            stack_generations: StackGenerations::new(1),
        }
    }
    pub fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        (slot == 0).then_some((&self.stack_generations, &self.item, 0))
    }
    pub fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        (slot == 0).then_some((&mut self.stack_generations, &mut self.item, 0))
    }
    fn replace_item(&mut self, item: ItemStack) {
        self.stack_generations.replace(0, &mut self.item, item);
    }
    /// Port of RandomizableContainer#setLootTable(table, seed): no item reset.
    pub fn set_loot_table(&mut self, table: Identifier, seed: i64) {
        self.loot_table = Some(table);
        self.loot_table_seed = seed;
    }
}
fn empty_decorations() -> PotDecorations {
    PotDecorations {
        back: None,
        left: None,
        right: None,
        front: None,
    }
}
/// TagValueInput#read(PotDecorations.CODEC) keeps each optional field's partial.
/// ItemStack's existing count-partial adapter recovers 1 without relaxing the
/// strict persistent template/component codecs.
pub(super) fn read_decorations(tag: &Tag) -> Option<PotDecorations> {
    let tag = tag.as_compound()?;
    let side = |name| {
        tag.get(name).and_then(|value| {
            ItemStackTemplate::from_nbt(value).ok().or_else(|| {
                let stack = ItemStack::from_nbt_with_partial(value).ok()?.0?;
                ItemStackTemplate::new(stack.item, stack.count, stack.components).ok()
            })
        })
    };
    Some(PotDecorations {
        back: side("back"),
        left: side("left"),
        right: side("right"),
        front: side("front"),
    })
}
pub(super) struct DecoratedPotBehavior;
pub(super) static DECORATED_POT_ENTITY: DecoratedPotBehavior = DecoratedPotBehavior;
impl BlockEntity for DecoratedPotBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::DecoratedPot(entity) = state else {
            unreachable!()
        };
        entity.decorations = tag
            .get("sherds")
            .and_then(read_decorations)
            .unwrap_or_else(empty_decorations);
        entity.loot_table = tag
            .get_string("LootTable")
            .and_then(|value| value.to_string_checked().ok())
            .and_then(|value| Identifier::parse(&value).ok());
        entity.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        let item = if entity.loot_table.is_some() {
            ItemStack::default()
        } else {
            tag.get("item")
                .and_then(|value| ItemStack::from_nbt_with_partial(value).ok())
                .unwrap_or_default()
        };
        entity.replace_item(item);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::DecoratedPot(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if entity.decorations != empty_decorations() {
            tag.insert("sherds", entity.decorations.to_nbt()?);
        }
        if let Some(table) = &entity.loot_table {
            tag.insert("LootTable", Tag::String(table.as_str().into()));
            if entity.loot_table_seed != 0 {
                tag.insert("LootTableSeed", Tag::Long(entity.loot_table_seed));
            }
        } else if !entity.item.is_empty() {
            tag.insert("item", entity.item.to_nbt()?);
        }
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        components: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::DecoratedPot(entity) = state else {
            unreachable!()
        };
        entity.decorations = match components.get(ComponentTypeId::POT_DECORATIONS) {
            Some(DataComponent::PotDecorations(value)) => value.clone(),
            _ => empty_decorations(),
        };
        entity.replace_item(storage::component_item(components, 0));
        &[ComponentTypeId::POT_DECORATIONS, ComponentTypeId::CONTAINER]
    }
    fn collect_implicit_components(
        &self,
        state: &BlockEntityState,
        components: &mut DataComponentMap,
    ) {
        let BlockEntityState::DecoratedPot(entity) = state else {
            unreachable!()
        };
        storage::set(
            components,
            DataComponent::PotDecorations(entity.decorations.clone()),
        );
        storage::set(
            components,
            storage::container_component(std::slice::from_ref(&entity.item)),
        );
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("sherds");
        tag.remove("item");
    }
}
