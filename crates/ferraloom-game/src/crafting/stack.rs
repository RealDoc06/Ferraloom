use super::Result;
use ferraloom_data::Item;
use ferraloom_protocol::component::{
    ComponentTypeId, DataComponent, DataComponentPatch, ItemStack, ItemStackTemplate,
};

pub(super) fn create(template: &ItemStackTemplate) -> ItemStack {
    validate_template(ItemStack::new(
        template.item,
        template.count,
        template.components.clone(),
    ))
}
pub(super) fn copy_with_count(stack: &ItemStack, count: i32) -> ItemStack {
    stack.copy_with_count(count)
}
pub(super) fn component(stack: &ItemStack, id: ComponentTypeId) -> Result<Option<DataComponent>> {
    Ok(stack.component(id)?.cloned())
}
pub(super) fn set(stack: &mut ItemStack, component: DataComponent) {
    if let Some(contents) = &mut stack.0 {
        contents.components.set(component);
        contents.components.sanitize(
            ferraloom_protocol::component::DataComponentMap::item_defaults(contents.item)
                .expect("typed registry prototype"),
        );
    }
}
pub(super) fn remove(stack: &mut ItemStack, id: ComponentTypeId) {
    if let Some(contents) = &mut stack.0 {
        contents.components.remove(id);
        contents.components.sanitize(
            ferraloom_protocol::component::DataComponentMap::item_defaults(contents.item)
                .expect("typed registry prototype"),
        );
    }
}
pub(super) fn item(stack: &ItemStack) -> Item {
    stack.item()
}
/// Port of ItemStackTemplate#apply: the template patch overrides the source patch.
pub(super) fn transmute(template: &ItemStackTemplate, source: &ItemStack, extra: i32) -> ItemStack {
    let mut patch = source.components_patch().clone();
    apply_patch(&mut patch, &template.components);
    validate_template(ItemStack::new(
        template.item,
        template.count.wrapping_add(extra),
        patch,
    ))
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
    Ok(a.is_same_item_same_components(b)?)
}

/// Port of ItemStackTemplate#validate: strict-invalid templates become EMPTY.
fn validate_template(stack: ItemStack) -> ItemStack {
    match crate::item::stack::validate_strict(&stack) {
        Ok(()) => stack,
        Err(crate::item::stack::ValidationError::Invalid(_)) => ItemStack::default(),
        Err(crate::item::stack::ValidationError::Protocol(error)) => {
            panic!("validated typed item defaults and components: {error}")
        }
    }
}
