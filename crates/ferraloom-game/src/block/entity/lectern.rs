//! Lectern persistence. Live book resolution and page/menu effects are required ports.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    entity::player::inventory::component,
    inventory::StackGenerations,
    level::{Level, LevelError},
};
use ferraloom_data::properties as p;
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, ItemStack, Persistent};

/// Port of LecternBlockEntity's book receiver, page and derived pageCount.
#[derive(Clone, Debug)]
pub struct LecternBlockEntity {
    pub parent: BlockEntityData,
    pub book: ItemStack,
    pub page: i32,
    pub page_count: i32,
    stack_generations: StackGenerations,
}
impl LecternBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            book: ItemStack::default(),
            page: 0,
            page_count: 0,
            stack_generations: StackGenerations::new(1),
        }
    }
    pub fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        (slot == 0).then_some((&self.stack_generations, &self.book, 0))
    }
    pub fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        (slot == 0).then_some((&mut self.stack_generations, &mut self.book, 0))
    }
    pub fn has_book(&self) -> bool {
        component(&self.book, ComponentTypeId::WRITTEN_BOOK_CONTENT).is_some()
            || component(&self.book, ComponentTypeId::WRITABLE_BOOK_CONTENT).is_some()
    }
    /// Port of getRedstoneSignal, retaining the native primitive float order.
    pub fn redstone_signal(&self) -> i32 {
        let progress = if self.page_count > 1 {
            self.page as f32 / (self.page_count as f32 - 1.0)
        } else {
            1.0
        };
        (progress * 14.0).floor() as i32 + i32::from(self.has_book())
    }
    fn load(&mut self, book: ItemStack, tag: &CompoundTag) {
        self.stack_generations.replace(0, &mut self.book, book);
        self.page_count = match component(&self.book, ComponentTypeId::WRITTEN_BOOK_CONTENT) {
            Some(DataComponent::WrittenBookContent(content)) => content.pages.len(),
            _ => match component(&self.book, ComponentTypeId::WRITABLE_BOOK_CONTENT) {
                Some(DataComponent::WritableBookContent(content)) => content.0.len(),
                _ => 0,
            },
        }
        .try_into()
        .expect("native book page count fits an int");
        // Mth.clamp uses min(max(value, 0), -1) for a book with zero pages.
        self.page =
            ferraloom_util::mth::clamp_int(tag.get_int_or("Page", 0), 0, self.page_count - 1);
    }
}
fn read_book(tag: &CompoundTag) -> ItemStack {
    tag.get("Book")
        .and_then(|value| ItemStack::from_nbt_with_partial(value).ok())
        .unwrap_or_default()
}
pub(super) struct LecternBehavior;
pub(super) static LECTERN_ENTITY: LecternBehavior = LecternBehavior;
impl BlockEntity for LecternBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Lectern(entity) = state else {
            unreachable!()
        };
        entity.load(read_book(tag), tag);
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let book = read_book(tag);
        if let Some(DataComponent::WrittenBookContent(content)) =
            component(&book, ComponentTypeId::WRITTEN_BOOK_CONTENT)
            && !content.resolved
            && level
                .block_entity::<LecternBlockEntity>(handle)?
                .parent
                .has_level
        {
            // resolveBook needs the lectern's CommandSourceStack and live text
            // resolution; never persist an unresolved result as if it ran.
            return Err(LevelError::UnsupportedBlockEntity(
                ferraloom_data::Block::LECTERN,
            ));
        }
        level
            .block_entity_mut::<LecternBlockEntity>(handle)?
            .load(book, tag);
        Ok(())
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Lectern(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if !entity.book.is_empty() {
            tag.insert("Book", entity.book.to_nbt()?);
            tag.insert("Page", ferraloom_nbt::Tag::Int(entity.page));
        }
        Ok(tag)
    }
    fn pre_remove_side_effects(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let entity = level.block_entity::<LecternBlockEntity>(handle)?;
        if entity.parent.has_level && entity.parent.block_state.get(p::HAS_BOOK) == Some(true) {
            return Err(LevelError::UnsupportedBlockEntity(
                ferraloom_data::Block::LECTERN,
            ));
        }
        Ok(())
    }
}
