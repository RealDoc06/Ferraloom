//! Immutable live-registry facts used by recipe matching. Saved-map and potion
//! effect queries remain explicit requirements until their world services land.
use super::*;
use crate::crafting::{CraftingContext, Error as CraftingError};
use ferraloom_data::datapack::{Identifier as ResourceId, ResourceManager, load_tags};
use ferraloom_protocol::{Identifier, codec::RegistryContext};
use std::collections::BTreeMap;

/// Runtime tag membership, loaded against the same registry snapshot as recipes.
/// This never substitutes an empty map store or a guessed enchantment curse set.
#[derive(Debug)]
pub struct RuntimeCraftingContext {
    enchantments: BTreeMap<ResourceId, Vec<Identifier>>,
    potions: BTreeMap<ResourceId, Vec<Identifier>>,
}
impl RuntimeCraftingContext {
    pub fn load(
        resources: &ResourceManager,
        registry: &impl RegistryContext,
    ) -> crate::crafting::Result<Self> {
        let load = |name: &str| -> crate::crafting::Result<_> {
            let registry_id = Identifier::parse(name)?;
            let tags = load_tags(resources, name, |id, _| {
                let key = Identifier::parse(&id.to_string()).ok()?;
                registry.entry_id(&registry_id, &key).map(|_| key)
            });
            if !tags.diagnostics.is_empty() {
                return Err(CraftingError::Invalid(format!(
                    "invalid {name} crafting tags: {:?}",
                    tags.diagnostics
                )));
            }
            Ok(tags.values)
        };
        Ok(Self {
            enchantments: load("enchantment")?,
            potions: load("potion")?,
        })
    }
}
impl CraftingContext for RuntimeCraftingContext {
    fn is_curse(&self, enchantment: &Identifier) -> crate::crafting::Result<bool> {
        let tag = "minecraft:curse".parse().expect("built-in tag");
        let values = self
            .enchantments
            .get(&tag)
            .ok_or(CraftingError::ContextRequired(
                "loaded enchantment curse tag",
            ))?;
        Ok(values.contains(enchantment))
    }
    fn potion_in_tag(
        &self,
        tag: &Identifier,
        potion: &Identifier,
    ) -> crate::crafting::Result<bool> {
        let key = tag.as_str().parse()?;
        let values = self
            .potions
            .get(&key)
            .ok_or(CraftingError::ContextRequired("loaded potion tag"))?;
        Ok(values.contains(potion))
    }
}

/// Server integration gate for the still-unported statistic, advancement and
/// item postprocessing callbacks. Result calculation/placement remain usable;
/// taking a result is rejected before any canonical inventory mutation.
pub struct PendingCraftingCallbacks;
impl CraftingCallbacks for PendingCraftingCallbacks {
    fn validate_take_result(&self) -> Result<()> {
        Err(MenuError::Unsupported(
            "crafting statistics and advancement callbacks",
        ))
    }
    fn item_enabled(&self, _stack: &ItemStack) -> Result<bool> {
        // Every registered 26.3 item is enabled by DEFAULT_FLAGS. The Java menu
        // fixture verifies this independently when loading the actual registry.
        Ok(true)
    }
    fn item_crafted(
        &mut self,
        _menu: &mut AbstractContainerMenu,
        _ctx: &mut MenuContext<'_>,
        _stack: &ItemStack,
        _amount: Option<i32>,
    ) -> Result<()> {
        self.validate_take_result()
    }
    fn recipe_crafted(
        &mut self,
        _menu: &mut AbstractContainerMenu,
        _ctx: &mut MenuContext<'_>,
        _recipe: &crate::crafting::RecipeHolder,
        _input: &[ItemStack],
    ) -> Result<()> {
        self.validate_take_result()
    }
    fn recipe_unlocked(
        &mut self,
        _menu: &mut AbstractContainerMenu,
        _ctx: &mut MenuContext<'_>,
        _recipe: &crate::crafting::RecipeHolder,
        _book: &crate::crafting::ServerRecipeBook,
    ) -> Result<()> {
        self.validate_take_result()
    }
}
