//! Canonical ItemStack#validateStrict; runtime validation is distinct from its codecs.
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, ItemStack, ItemStackTemplate};
use std::fmt;

#[derive(Debug)]
pub enum ValidationError {
    Invalid(String),
    Protocol(ferraloom_protocol::Error),
}
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => f.write_str(reason),
            Self::Protocol(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}
impl From<ferraloom_protocol::Error> for ValidationError {
    fn from(error: ferraloom_protocol::Error) -> Self {
        Self::Protocol(error)
    }
}
fn maximum(stack: &ItemStack) -> std::result::Result<i32, ValidationError> {
    Ok(match stack.component(ComponentTypeId::MAX_STACK_SIZE)? {
        Some(DataComponent::MaxStackSize(value)) => value.0,
        _ => 1,
    })
}
fn contained<'a>(
    items: impl IntoIterator<Item = &'a ItemStackTemplate>,
) -> std::result::Result<(), ValidationError> {
    for item in items {
        let stack = ItemStack::new(item.item, item.count, item.components.clone());
        let maximum = maximum(&stack)?;
        if item.count > maximum {
            return Err(ValidationError::Invalid(format!(
                "Item stack with count of {} was larger than maximum: {maximum}",
                item.count
            )));
        }
    }
    Ok(())
}
/// Port of ItemStack#validateStrict and #validateComponents. Retain native check
/// order, including contained-size and bundle-weight errors before the own count.
pub fn validate_strict(stack: &ItemStack) -> std::result::Result<(), ValidationError> {
    let maximum = maximum(stack)?;
    if stack.component(ComponentTypeId::MAX_DAMAGE)?.is_some() && maximum > 1 {
        return Err(ValidationError::Invalid(
            "Item cannot be both damageable and stackable".into(),
        ));
    }
    if let Some(DataComponent::Container(items)) = stack.component(ComponentTypeId::CONTAINER)? {
        contained(items.0.iter().flatten())?;
    }
    if let Some(DataComponent::BundleContents(items)) =
        stack.component(ComponentTypeId::BUNDLE_CONTENTS)?
    {
        contained(items)?;
        if bundle_weight(items)?.is_none() {
            return Err(ValidationError::Invalid(
                "Excessive total bundle weight".into(),
            ));
        }
    }
    if let Some(DataComponent::ChargedProjectiles(items)) =
        stack.component(ComponentTypeId::CHARGED_PROJECTILES)?
    {
        contained(&items.0)?;
    }
    let count = stack
        .0
        .as_ref()
        .filter(|_| !stack.is_empty())
        .map_or(0, |s| s.count);
    if count > maximum {
        return Err(ValidationError::Invalid(format!(
            "Item stack with stack size of {count} was larger than maximum: {maximum}"
        )));
    }
    Ok(())
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
fn bundle_weight(
    items: &[ItemStackTemplate],
) -> std::result::Result<Option<(i32, i32)>, ValidationError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{arguments::items::ItemInput, exceptions::error_component};
    use ferraloom_data::Item;
    use ferraloom_protocol::component::Persistent;

    #[test]
    fn strict_validation_and_item_input_reasons_match_native() {
        let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../commands/vanilla/item-validation-vectors.json"
        ))
        .unwrap();
        for row in &rows {
            let item = Item::from_name(row["input"]["id"].as_str().unwrap()).unwrap();
            let source = if item == Item::AIR {
                ItemStack::default()
            } else {
                ItemStack::from_json(&row["input"]).unwrap()
            };
            let count = row["count"].as_i64().unwrap() as i32;
            let value = source.copy_with_count(count);
            let reason = validate_strict(&value).err().map(|e| e.to_string());
            assert_eq!(
                reason.as_deref(),
                row.get("reason").and_then(serde_json::Value::as_str),
                "{row}"
            );
            let input = ItemInput {
                item,
                components: source.components_patch().clone(),
            };
            let error = input.create_item_stack(count).err().map(|error| {
                serde_json::from_str::<serde_json::Value>(
                    &error_component(&error).to_json().unwrap(),
                )
                .unwrap()
            });
            assert_eq!(error.as_ref(), row.get("input_error"), "{row}");
        }
        assert_eq!(rows.len(), 96);
    }
}
