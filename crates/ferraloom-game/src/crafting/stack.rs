use super::Result;
use ferraloom_data::Item;
use ferraloom_protocol::component::{
    ComponentTypeId, DataComponent, DataComponentPatch, ItemStack, ItemStackTemplate,
};

pub(super) fn create(template: &ItemStackTemplate) -> ItemStack {
    ItemStack::new(template.item, template.count, template.components.clone())
}
pub(super) fn copy_with_count(stack: &ItemStack, count: i32) -> ItemStack {
    stack.0.as_ref().map_or_else(ItemStack::default, |stack| {
        ItemStack::new(stack.item, count, stack.components.clone())
    })
}
pub(super) fn component(stack: &ItemStack, id: ComponentTypeId) -> Result<Option<DataComponent>> {
    Ok(stack.components()?.get(id).cloned())
}
pub(super) fn set(stack: &mut ItemStack, component: DataComponent) {
    if let Some(contents) = &mut stack.0 {
        contents.components.set(component);
    }
}
pub(super) fn remove(stack: &mut ItemStack, id: ComponentTypeId) {
    if let Some(contents) = &mut stack.0 {
        contents.components.remove(id);
    }
}
pub(super) fn item(stack: &ItemStack) -> Item {
    stack.0.as_ref().map_or(Item::AIR, |stack| stack.item)
}
/// Port of ItemStackTemplate#apply: the template patch overrides the source patch.
pub(super) fn transmute(template: &ItemStackTemplate, source: &ItemStack, extra: i32) -> ItemStack {
    let mut patch = source
        .0
        .as_ref()
        .map(|stack| stack.components.clone())
        .unwrap_or_default();
    apply_patch(&mut patch, &template.components);
    ItemStack::new(template.item, template.count + extra, patch)
}
pub(super) fn apply_patch(target: &mut DataComponentPatch, patch: &DataComponentPatch) {
    for key in &patch.removed {
        target.remove(*key);
    }
    for value in &patch.added {
        target.set(value.clone());
    }
}
pub(super) fn same_item_components(a: &ItemStack, b: &ItemStack) -> Result<bool> {
    if a.is_empty() || b.is_empty() {
        return Ok(a.is_empty() && b.is_empty());
    }
    let a_components = a.components()?;
    let b_components = b.components()?;
    Ok(item(a) == item(b)
        && a_components.entries.len() == b_components.entries.len()
        && a_components
            .entries
            .iter()
            .all(|entry| b_components.get(entry.component_type()) == Some(entry)))
}
