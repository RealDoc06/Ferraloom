//! Port of Item#getName, its component-dependent overrides and PotionContents#getName.
use ferraloom_data::{
    Item,
    registries::{data_component_type::Id as ComponentType, potion},
};
use ferraloom_protocol::{
    Error, Result,
    chat::{Argument, Component},
    codec::Either,
    component::{DataComponent, DataComponentMap, ItemStack, Persistent},
};

/// Port of ItemStack#getItemName. The effective item is AIR for empty stacks,
/// whose AirItem override reads the holder prototype instead of stack components.
pub fn get_item_name(stack: &ItemStack) -> Result<Component> {
    let item = stack.item();
    if item == Item::AIR {
        return Ok(base_name(
            DataComponentMap::item_defaults(Item::AIR)?.get(ComponentType::ITEM_NAME),
        ));
    }
    if item == Item::COMPASS && stack.component(ComponentType::LODESTONE_TRACKER)?.is_some() {
        return Ok(Component::translatable(
            "item.minecraft.lodestone_compass",
            Vec::new(),
        ));
    }
    if item == Item::SHIELD
        && let Some(DataComponent::BaseColor(color)) = stack.component(ComponentType::BASE_COLOR)?
    {
        let color = color.to_json()?;
        return Ok(Component::translatable(
            format!(
                "{}.{}",
                item.description_id(),
                color.as_str().expect("native dye name")
            ),
            Vec::new(),
        ));
    }
    if matches!(
        item,
        Item::POTION | Item::SPLASH_POTION | Item::LINGERING_POTION | Item::TIPPED_ARROW
    ) && let Some(DataComponent::PotionContents(contents)) =
        stack.component(ComponentType::POTION_CONTENTS)?
    {
        let potion = contents
            .potion
            .as_ref()
            .map(|potion| {
                // Registry-aware decode (5a) will reject this before the read.
                potion::from_name(potion.0.as_str())
                    .map(|potion| potion.effect_name())
                    .ok_or_else(|| Error::invalid("potion registry entry", potion.0.to_string()))
            })
            .transpose()?;
        let suffix = contents
            .custom_name
            .as_deref()
            .or(potion)
            .unwrap_or("empty");
        return Ok(Component::translatable(
            format!("{}.effect.{suffix}", item.description_id()),
            Vec::new(),
        ));
    }
    if item == Item::PLAYER_HEAD
        && let Some(DataComponent::Profile(profile)) = stack.component(ComponentType::PROFILE)?
    {
        let name = match &profile.profile {
            Either::Left(profile) => Some(profile.name.0.as_str()),
            Either::Right(profile) => profile.name.as_ref().map(|name| name.0.as_str()),
        };
        if let Some(name) = name {
            return Ok(Component::translatable(
                format!("{}.named", item.description_id()),
                vec![Argument::String(name.into())],
            ));
        }
    }
    Ok(base_name(stack.component(ComponentType::ITEM_NAME)?))
}
fn base_name(component: Option<&DataComponent>) -> Component {
    match component {
        Some(DataComponent::ItemName(name)) => (**name).clone(),
        _ => Component::default(),
    }
}

/// Extension of the canonical protocol stack; all callers use the same helper.
pub trait ItemStackName {
    fn get_item_name(&self) -> Result<Component>;
}
impl ItemStackName for ItemStack {
    fn get_item_name(&self) -> Result<Component> {
        get_item_name(self)
    }
}

#[cfg(test)]
mod tests;
