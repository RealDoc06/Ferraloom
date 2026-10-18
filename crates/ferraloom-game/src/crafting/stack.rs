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

/// Port of ItemStackTemplate#validate / ItemStack#validateStrict. These runtime
/// checks are distinct from persistent codecs, which only bound counts to 1..99.
fn validate_template(stack: ItemStack) -> ItemStack {
    let valid = (|| -> Result<bool> {
        let Some(contents) = &stack.0 else {
            return Ok(true);
        };
        let components = stack.components()?;
        let maximum = match components.get(ComponentTypeId::MAX_STACK_SIZE) {
            Some(DataComponent::MaxStackSize(v)) => v.0,
            _ => 1,
        };
        if contents.count > maximum
            || components.get(ComponentTypeId::MAX_DAMAGE).is_some() && maximum > 1
        {
            return Ok(false);
        }
        for component in &components.entries {
            let items: Vec<_> = match component {
                DataComponent::Container(items) => items.0.iter().flatten().collect(),
                DataComponent::BundleContents(items) => items.iter().collect(),
                DataComponent::ChargedProjectiles(items) => items.0.iter().collect(),
                _ => Vec::new(),
            };
            for item in items {
                let item_stack = ItemStack::new(item.item, item.count, item.components.clone());
                let maximum = match item_stack
                    .components()?
                    .get(ComponentTypeId::MAX_STACK_SIZE)
                {
                    Some(DataComponent::MaxStackSize(v)) => v.0,
                    _ => 1,
                };
                if item.count > maximum {
                    return Ok(false);
                }
            }
            if let DataComponent::BundleContents(items) = component
                && bundle_weight(items)?.is_none()
            {
                return Ok(false);
            }
        }
        Ok(true)
    })()
    .expect("validated typed item defaults and components");
    if valid { stack } else { ItemStack::default() }
}
// Apache Fraction uses signed 32-bit numerators/denominators, reducing before
// overflow checks. The checked arithmetic here follows that representable range.
fn fraction(n: i64, d: i64) -> Option<(i32, i32)> {
    fn gcd(mut a: i64, mut b: i64) -> i64 {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a.abs()
    }
    let gcd = gcd(n, d);
    Some((i32::try_from(n / gcd).ok()?, i32::try_from(d / gcd).ok()?))
}
fn bundle_weight(items: &[ItemStackTemplate]) -> Result<Option<(i32, i32)>> {
    let mut weight = (0i32, 1i32);
    for item in items {
        let item_stack = ItemStack::new(item.item, item.count, item.components.clone());
        let components = item_stack.components()?;
        let single = if let Some(DataComponent::BundleContents(items)) =
            components.get(ComponentTypeId::BUNDLE_CONTENTS)
        {
            let Some((n, d)) = bundle_weight(items)? else {
                return Ok(None);
            };
            fraction(i64::from(n) * 16 + i64::from(d), i64::from(d) * 16)
        } else if matches!(components.get(ComponentTypeId::BEES),Some(DataComponent::Bees(bees)) if !bees.is_empty())
        {
            Some((1, 1))
        } else {
            let maximum = match components.get(ComponentTypeId::MAX_STACK_SIZE) {
                Some(DataComponent::MaxStackSize(v)) => v.0,
                _ => 1,
            };
            Some((1, maximum))
        };
        let Some((n, d)) =
            single.and_then(|(n, d)| fraction(i64::from(n) * i64::from(item.count), i64::from(d)))
        else {
            return Ok(None);
        };
        let Some(next) = fraction(
            i64::from(weight.0) * i64::from(d) + i64::from(n) * i64::from(weight.1),
            i64::from(weight.1) * i64::from(d),
        ) else {
            return Ok(None);
        };
        weight = next;
    }
    Ok(Some(weight))
}
