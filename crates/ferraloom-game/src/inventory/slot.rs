use super::{Container, ItemStack, MenuError, Result, count, max_stack_size};
use ferraloom_data::Item;
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, EquipmentSlot};

/// Stable references into canonical storage. External containers are borrowed per operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContainerId {
    Player,
    Local(usize),
    External(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultKind {
    Crafting,
    Furnace,
    Merchant,
    Anvil,
    Grindstone,
    Stonecutter,
    Loom,
    Cartography,
    Smithing,
}

/// Port of Slot subclasses' placement/pickup rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Normal,
    GrindstoneInput,
    NonInteractiveResult,
    Armor(EquipmentSlot),
    Offhand,
    Result(ResultKind),
    FurnaceFuel,
    ShulkerBox,
    BrewingBottle,
    BrewingIngredient,
    BrewingFuel,
    BeaconPayment,
    EnchantmentItem,
    Lapis,
    Banner,
    Dye,
    BannerPattern,
    Map,
    MapAddition,
    SmithingTemplate,
    SmithingBase,
    SmithingAddition,
    Crafter,
    MountEquipment(EquipmentSlot),
    Lectern,
}

/// Pure domain hooks. Default errors make unported recipe, enchantment and trade work explicit.
/// Implementations may hold immutable recipe/fuel tables; they do not own player stacks.
pub trait MenuHooks {
    /// Live counterparts keep canonical receivers addressable across callbacks.
    fn live_grid_changed(
        &mut self,
        _player: crate::entity::EntityRef,
        _menu: super::MenuRef,
        _level: &mut crate::level::Level,
    ) -> Result<()> {
        Err(MenuError::Unsupported("live crafting result calculation"))
    }
    fn validate_live_result(&self, _kind: ResultKind) -> Result<()> {
        Err(MenuError::Unsupported("live result-slot achievements"))
    }
    fn live_grid_changed_with_hint(
        &mut self,
        player: crate::entity::EntityRef,
        menu: super::MenuRef,
        _hint: &ferraloom_data::datapack::Identifier,
        level: &mut crate::level::Level,
    ) -> Result<()> {
        self.live_grid_changed(player, menu, level)
    }
    fn live_item_crafted(
        &mut self,
        _player: crate::entity::EntityRef,
        _menu: super::MenuRef,
        _stack: super::StackRef,
        _amount: i32,
        _level: &mut crate::level::Level,
    ) -> Result<()> {
        Err(MenuError::Unsupported("crafting statistic producer"))
    }
    fn live_award_recipe(
        &mut self,
        _player: crate::entity::EntityRef,
        _menu: super::MenuRef,
        _recipe: &ferraloom_data::datapack::Identifier,
        _input: &[super::StackRef],
        _level: &mut crate::level::Level,
    ) -> Result<bool> {
        Err(MenuError::Unsupported("crafting advancement producer"))
    }
    fn live_award_furnace(
        &mut self,
        _player: crate::entity::EntityRef,
        _furnace: crate::block::entity::BlockEntityRef,
        _level: &mut crate::level::Level,
    ) -> Result<()> {
        Err(MenuError::Unsupported(
            "furnace experience and recipe producer",
        ))
    }
    fn crafting_ref(&self) -> Option<&dyn super::CraftingMenuHooks> {
        None
    }
    fn crafting(&mut self) -> Option<&mut dyn super::CraftingMenuHooks> {
        None
    }
    fn has_crafting(&self) -> bool {
        false
    }

    fn prepare_recipe_click(&mut self, _kind: super::MenuKind) -> Result<()> {
        Err(MenuError::Unsupported(
            "synchronous recipe/enchantment/trade calculation",
        ))
    }
    fn may_place(&self, kind: SlotKind, _stack: &ItemStack) -> Result<bool> {
        Err(MenuError::Unsupported(match kind {
            SlotKind::FurnaceFuel => "furnace fuel registry",
            SlotKind::BrewingIngredient => "brewing recipes",
            SlotKind::SmithingTemplate | SlotKind::SmithingBase | SlotKind::SmithingAddition => {
                "smithing recipe property sets"
            }
            SlotKind::MountEquipment(_) => "mount equipment predicate",
            _ => "specialized slot predicate",
        }))
    }
    fn may_pickup_armor(&self, stack: &ItemStack) -> Result<bool> {
        if !crate::entity::player::inventory::is_enchanted(stack) {
            return Ok(true);
        }
        Err(MenuError::Unsupported(
            "PREVENT_ARMOR_CHANGE enchantment effect",
        ))
    }
    fn result_available(&self, _kind: ResultKind, _stack: &ItemStack) -> Result<bool> {
        Err(MenuError::Unsupported(
            "result-slot recipe/enchantment/trade hook",
        ))
    }
    fn on_take(
        &mut self,
        _kind: ResultKind,
        _stack: &ItemStack,
        _quick_craft_count: i32,
    ) -> Result<()> {
        Err(MenuError::Unsupported(
            "result-slot consumption and achievements hook",
        ))
    }
    fn accepts_stonecutter(&self, _stack: &ItemStack) -> Result<bool> {
        Err(MenuError::Unsupported("stonecutter recipes"))
    }
    fn can_smelt(&self, _stack: &ItemStack) -> Result<bool> {
        Err(MenuError::Unsupported("cooking recipe property set"))
    }
    fn click_button(&mut self, _button: i32) -> Result<bool> {
        Err(MenuError::Unsupported(
            "menu button recipe/enchantment/trade hook",
        ))
    }
    fn slots_changed(&mut self, _kind: super::MenuKind, _items: &[ItemStack]) -> Result<()> {
        Err(MenuError::Unsupported("recipe result calculation"))
    }
}
#[derive(Debug, Default)]
pub struct UnsupportedMenuHooks;
impl MenuHooks for UnsupportedMenuHooks {}

/// Port of Slot. State is a storage address plus immutable virtual-method parameters.
#[derive(Clone, Debug)]
pub struct Slot {
    pub container: ContainerId,
    pub container_slot: usize,
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub kind: SlotKind,
    pub active: bool,
    pub disabled: bool,
}
impl Slot {
    pub fn new(container: ContainerId, slot: usize, x: i32, y: i32) -> Self {
        Self {
            container,
            container_slot: slot,
            index: 0,
            x,
            y,
            kind: SlotKind::Normal,
            active: true,
            disabled: false,
        }
    }
    pub fn with_kind(mut self, kind: SlotKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn may_place(&self, stack: &ItemStack, hooks: &dyn MenuHooks) -> Result<bool> {
        let item = Some(stack.item());
        Ok(match self.kind {
            SlotKind::Normal | SlotKind::Offhand | SlotKind::EnchantmentItem => true,
            SlotKind::Banner => item.is_some_and(|i| i.name().ends_with("_banner")),
            SlotKind::GrindstoneInput => {
                let c = stack.components()?;
                (c.get(ComponentTypeId::MAX_DAMAGE).is_some() && c.get(ComponentTypeId::DAMAGE).is_some() && c.get(ComponentTypeId::UNBREAKABLE).is_none()) || [ComponentTypeId::ENCHANTMENTS,ComponentTypeId::STORED_ENCHANTMENTS].into_iter().any(|key| matches!(c.get(key),Some(DataComponent::Enchantments(v)|DataComponent::StoredEnchantments(v)) if !v.0.is_empty()))
            }
            SlotKind::Result(_) | SlotKind::NonInteractiveResult => false,
            SlotKind::Crafter => !self.disabled,
            SlotKind::Armor(slot) => match crate::entity::player::inventory::component(
                stack,
                ComponentTypeId::EQUIPPABLE,
            ) {
                Some(DataComponent::Equippable(value)) if value.slot == slot => {
                    match &value.allowed_entities {
                        None => true,
                        Some(allowed) => match &allowed.0 {
                            ferraloom_protocol::codec::HolderSet::Direct(types) => types
                                .iter()
                                .any(|entity| entity.as_str() == "minecraft:player"),
                            ferraloom_protocol::codec::HolderSet::Named(_) => {
                                hooks.may_place(self.kind, stack)?
                            }
                        },
                    }
                }
                _ => false,
            },
            SlotKind::ShulkerBox => !item.is_some_and(|i| i.name().ends_with("shulker_box")),
            SlotKind::FurnaceFuel => {
                stack
                    .components()?
                    .get(ComponentTypeId::COOKING_FUEL)
                    .is_some()
                    || item == Some(Item::BUCKET)
            }
            SlotKind::BrewingBottle => hooks.may_place(self.kind, stack)?,
            SlotKind::BrewingFuel => stack
                .components()?
                .get(ComponentTypeId::BREWING_FUEL)
                .is_some(),
            SlotKind::Lapis => item == Some(Item::LAPIS_LAZULI),
            SlotKind::Dye => stack.components()?.get(ComponentTypeId::DYE).is_some(),
            SlotKind::BannerPattern => stack
                .components()?
                .get(ComponentTypeId::PROVIDES_BANNER_PATTERNS)
                .is_some(),
            SlotKind::Map => stack.components()?.get(ComponentTypeId::MAP_ID).is_some(),
            SlotKind::MapAddition => {
                matches!(item, Some(Item::PAPER | Item::MAP | Item::GLASS_PANE))
            }
            _ => hooks.may_place(self.kind, stack)?,
        })
    }
    pub fn may_pickup(
        &self,
        stack: &ItemStack,
        creative: bool,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        match self.kind {
            SlotKind::NonInteractiveResult => Ok(false),
            SlotKind::Armor(_) if !creative && !stack.is_empty() => hooks.may_pickup_armor(stack),
            SlotKind::Result(kind) if !stack.is_empty() => hooks.result_available(kind, stack),
            _ => Ok(true),
        }
    }
    fn fixed_stack_limit(&self) -> Option<i32> {
        match self.kind {
            SlotKind::Armor(_)
            | SlotKind::BrewingBottle
            | SlotKind::EnchantmentItem
            | SlotKind::BeaconPayment
            | SlotKind::MountEquipment(_) => Some(1),
            _ => None,
        }
    }
    pub fn get_max_stack_size(&self, container: &dyn Container) -> i32 {
        self.fixed_stack_limit()
            .unwrap_or_else(|| container.max_stack_size())
    }
    pub(crate) fn max_stack_size_with_limit(
        &self,
        stack: &ItemStack,
        container_limit: impl FnOnce() -> Result<i32>,
    ) -> Result<i32> {
        if self.kind == SlotKind::FurnaceFuel && stack.item() == Item::BUCKET {
            return Ok(1);
        }
        let limit = match self.fixed_stack_limit() {
            Some(limit) => limit,
            None => container_limit()?,
        };
        Ok(limit.min(max_stack_size(stack)))
    }
    pub fn max_stack_size(&self, container: &dyn Container, stack: &ItemStack) -> i32 {
        self.max_stack_size_with_limit(stack, || Ok(container.max_stack_size()))
            .expect("pure container stack limit")
    }
    pub fn allow_modification(
        &self,
        container: &dyn Container,
        creative: bool,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        let stack = container.get_item(self.container_slot);
        Ok(self.may_pickup(&stack, creative, hooks)? && self.may_place(&stack, hooks)?)
    }
    pub fn try_remove(
        &self,
        container: &mut dyn Container,
        amount: i32,
        max_amount: i32,
        creative: bool,
        hooks: &dyn MenuHooks,
    ) -> Result<Option<ItemStack>> {
        let stack = container.get_item(self.container_slot);
        if !self.may_pickup(&stack, creative, hooks)?
            || (!self.allow_modification(container, creative, hooks)? && max_amount < count(&stack))
        {
            return Ok(None);
        }
        let result = container.remove_item(self.container_slot, amount.min(max_amount));
        if result.is_empty() {
            return Ok(None);
        }
        if container.get_item(self.container_slot).is_empty() {
            self.set_by_player(container, ItemStack::default());
        }
        Ok(Some(result))
    }
    pub fn safe_take(
        &self,
        container: &mut dyn Container,
        amount: i32,
        max_amount: i32,
        creative: bool,
        hooks: &mut dyn MenuHooks,
    ) -> Result<ItemStack> {
        let result = self
            .try_remove(container, amount, max_amount, creative, hooks)?
            .unwrap_or_default();
        if !result.is_empty() {
            self.on_take(container, &result, 0, hooks)?;
        }
        Ok(result)
    }
    pub fn safe_insert(
        &self,
        container: &mut dyn Container,
        input: ItemStack,
        amount: i32,
        hooks: &dyn MenuHooks,
    ) -> Result<ItemStack> {
        super::transfer::safe_insert_container_item_stack(self, container, input, amount, hooks)
    }
    pub fn safe_clone(&self, container: &dyn Container) -> ItemStack {
        let stack = container.get_item(self.container_slot);
        super::with_count(&stack, max_stack_size(&stack))
    }
    pub fn set(&self, container: &mut dyn Container, stack: ItemStack) {
        container.set_item(self.container_slot, stack);
        container.set_changed();
    }
    pub fn set_by_player(&self, container: &mut dyn Container, stack: ItemStack) {
        self.set(container, stack);
    }
    pub fn on_take(
        &self,
        container: &mut dyn Container,
        stack: &ItemStack,
        quick_craft_count: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        if let SlotKind::Result(kind) = self.kind {
            hooks.on_take(kind, stack, quick_craft_count)?;
        }
        container.set_changed();
        Ok(())
    }
}
