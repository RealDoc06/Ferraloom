//! Inventory#placeItemBackInInventory shared by menu removal and recipe grid
//! clearing. Held receivers stay rooted through each insertion and packet send.
use super::*;

pub(super) trait InventoryReturnContext: MenuClickContext {
    type DroppedAddress: Copy;
    fn read_return_stack(&self, stack: Self::Stack) -> Result<&ItemStack>;
    fn inventory_return_slot(&self, stack: Self::Stack) -> Result<Option<usize>>;
    fn add_at_slot(&mut self, slot: usize, stack: Self::Stack) -> Result<bool>;
    fn drop_for_return(&mut self, stack: Self::Stack) -> Result<Self::DroppedAddress>;
}

/// Port of Inventory#placeItemBackInInventory. The send callback runs at each
/// native successful-add point; ServerPlaceRecipe supplies the native no-send.
pub(super) fn place_item_back<C: InventoryReturnContext>(
    context: &mut C,
    stack: C::Stack,
    mut send_slot: impl FnMut(&mut C, usize) -> Result<()>,
) -> Result<Option<C::DroppedAddress>> {
    while !context.read_return_stack(stack)?.is_empty() {
        let Some(slot) = context.inventory_return_slot(stack)? else {
            return context.drop_for_return(stack).map(Some);
        };
        let target = context.inventory_stack(slot)?;
        let space = max_stack_size(context.read_return_stack(stack)?)
            - count(context.read_return_stack(target)?);
        let split = context.split_stack(stack, space)?;
        let split = context.own_stack(split)?;
        if context.add_at_slot(slot, split)? {
            send_slot(context, slot)?;
        }
    }
    Ok(None)
}
