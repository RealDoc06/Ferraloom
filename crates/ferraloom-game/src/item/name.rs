//! Port of Item#getName, its component-dependent overrides and PotionContents#getName.
use ferraloom_data::{
    Item,
    registries::{data_component_type::Id as ComponentType, potion},
};
use ferraloom_protocol::{
    Error, Result,
    chat::{Argument, Component, HoverEvent, TextColor},
    codec::Either,
    component::{DataComponent, DataComponentMap, ItemStack, Persistent, Rarity},
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

/// Port of ItemStack#getCustomName, including the unfiltered written-book title.
fn custom_name(stack: &ItemStack) -> Result<Option<Component>> {
    if let Some(DataComponent::CustomName(name)) = stack.component(ComponentType::CUSTOM_NAME)? {
        return Ok(Some((**name).clone()));
    }
    if let Some(DataComponent::WrittenBookContent(book)) =
        stack.component(ComponentType::WRITTEN_BOOK_CONTENT)?
    {
        let title = &book.title.raw.0;
        // StringUtil#isWhitespace is Character.isWhitespace || isSpaceChar.
        if !title.chars().all(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{001c}'..='\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')) {
            return Ok(Some(Component::literal(title)));
        }
    }
    Ok(None)
}

/// Port of ItemStack#getHoverName. Custom names precede the item's virtual name.
pub fn get_hover_name(stack: &ItemStack) -> Result<Component> {
    match custom_name(stack)? {
        Some(name) => Ok(name),
        None => get_item_name(stack),
    }
}

/// Port of ItemStack#getRarity, including the enchantment promotion.
fn rarity(stack: &ItemStack) -> Result<Rarity> {
    let base = match stack.component(ComponentType::RARITY)? {
        Some(DataComponent::Rarity(value)) => *value,
        _ => Rarity::Common,
    };
    let enchanted = matches!(stack.component(ComponentType::ENCHANTMENTS)?, Some(DataComponent::Enchantments(value)) if !value.0.is_empty());
    Ok(if enchanted {
        match base {
            Rarity::Common | Rarity::Uncommon => Rarity::Rare,
            Rarity::Rare | Rarity::Epic => Rarity::Epic,
        }
    } else {
        base
    })
}

/// Port of ItemStack#getDisplayName: bracketed name, rarity, custom-name italics
/// and a ShowItem hover containing the actual count and component patch.
pub fn get_display_name(stack: &ItemStack) -> Result<Component> {
    let mut name = Component::default();
    name.extra.push(get_hover_name(stack)?);
    if stack.component(ComponentType::CUSTOM_NAME)?.is_some() {
        name.style.italic = Some(true);
    }
    let mut result = Component::translatable(
        "chat.square_brackets",
        vec![Argument::Component(Box::new(name))],
    );
    if !stack.is_empty() {
        result.style.color = Some(TextColor::parse(match rarity(stack)? {
            Rarity::Common => "white",
            Rarity::Uncommon => "yellow",
            Rarity::Rare => "aqua",
            Rarity::Epic => "light_purple",
        })?);
        let components = stack.components_patch().to_nbt()?;
        result.style.hover_event = Some(HoverEvent::ShowItem {
            item: stack.item().name().parse()?,
            count: stack
                .0
                .as_ref()
                .expect("nonempty stack")
                .count
                .try_into()
                .map_err(|_| {
                    Error::invalid("item hover count", "outside protocol representation")
                })?,
            components: components.as_compound().cloned().ok_or_else(|| {
                Error::invalid("item hover", "component patch must be a compound")
            })?,
        });
    }
    Ok(result)
}

/// Extension of the canonical protocol stack; all callers use the same helper.
pub trait ItemStackName {
    fn get_item_name(&self) -> Result<Component>;
    fn get_hover_name(&self) -> Result<Component>;
    fn get_display_name(&self) -> Result<Component>;
}
impl ItemStackName for ItemStack {
    fn get_item_name(&self) -> Result<Component> {
        get_item_name(self)
    }
    fn get_hover_name(&self) -> Result<Component> {
        get_hover_name(self)
    }
    fn get_display_name(&self) -> Result<Component> {
        get_display_name(self)
    }
}

#[cfg(test)]
mod tests;
