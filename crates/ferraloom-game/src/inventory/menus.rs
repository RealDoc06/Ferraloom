use super::*;
use crate::entity::player::inventory::component;
use ferraloom_data::Item;
use ferraloom_protocol::{
    component::{ComponentTypeId, DataComponent, EquipmentSlot},
    play::MenuId,
};

/// Port of MenuType's 25 registry entries, InventoryMenu, and the two mount menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Inventory,
    Chest { rows: u8 },
    Dispenser,
    Crafter,
    Anvil,
    Beacon,
    BlastFurnace,
    BrewingStand,
    Crafting,
    Enchantment,
    Furnace,
    Grindstone,
    Hopper,
    Lectern,
    Loom,
    Merchant,
    ShulkerBox,
    Smithing,
    Smoker,
    Cartography,
    Stonecutter,
    Horse { columns: u8 },
    Nautilus,
}
impl MenuKind {
    pub fn registry_id(self) -> Option<MenuId> {
        Some(MenuId(match self {
            Self::Chest { rows: 1..=6 } => {
                if let Self::Chest { rows } = self {
                    i32::from(rows) - 1
                } else {
                    unreachable!()
                }
            }
            Self::Dispenser => 6,
            Self::Crafter => 7,
            Self::Anvil => 8,
            Self::Beacon => 9,
            Self::BlastFurnace => 10,
            Self::BrewingStand => 11,
            Self::Crafting => 12,
            Self::Enchantment => 13,
            Self::Furnace => 14,
            Self::Grindstone => 15,
            Self::Hopper => 16,
            Self::Lectern => 17,
            Self::Loom => 18,
            Self::Merchant => 19,
            Self::ShulkerBox => 20,
            Self::Smithing => 21,
            Self::Smoker => 22,
            Self::Cartography => 23,
            Self::Stonecutter => 24,
            _ => return None,
        }))
    }
}
impl AbstractContainerMenu {
    /// Port of each concrete menu constructor's slot coordinates and order.
    /// Local storage is useful for player-owned grids and vanilla's unbound constructors.
    /// Block/mount providers bind their canonical storage through bind_external.
    pub fn new(kind: MenuKind, id: i32) -> Result<Self> {
        let mut menu = Self::empty(kind, id);
        let local = ContainerId::Local(0);
        let result = ContainerId::Local(1);
        let mut inventory_xy = (8, 84);
        match kind {
            MenuKind::Inventory => {
                menu.local = vec![SimpleContainer::new(4), SimpleContainer::new(4)];
                menu.add_slot(
                    Slot::new(result, 0, 154, 28).with_kind(SlotKind::Result(ResultKind::Crafting)),
                );
                menu.grid(local, 2, 2, 98, 18, SlotKind::Normal);
                for (i, equipment) in [
                    EquipmentSlot::Head,
                    EquipmentSlot::Chest,
                    EquipmentSlot::Legs,
                    EquipmentSlot::Feet,
                ]
                .into_iter()
                .enumerate()
                {
                    menu.add_slot(
                        Slot::new(ContainerId::Player, 39 - i, 8, 8 + i as i32 * 18)
                            .with_kind(SlotKind::Armor(equipment)),
                    );
                }
            }
            MenuKind::Chest { rows: 1..=6 } => {
                let MenuKind::Chest { rows } = kind else {
                    unreachable!()
                };
                menu.local.push(SimpleContainer::new(usize::from(rows) * 9));
                menu.grid(local, 9, usize::from(rows), 8, 18, SlotKind::Normal);
                inventory_xy.1 = 18 + i32::from(rows) * 18 + 13;
            }
            MenuKind::Dispenser | MenuKind::Crafter => {
                menu.local = vec![SimpleContainer::new(9), SimpleContainer::new(4)];
                menu.grid(
                    local,
                    3,
                    3,
                    if kind == MenuKind::Crafter { 26 } else { 62 },
                    17,
                    if kind == MenuKind::Crafter {
                        SlotKind::Crafter
                    } else {
                        SlotKind::Normal
                    },
                );
                if kind == MenuKind::Crafter {
                    menu.add_data(10);
                }
            }
            MenuKind::ShulkerBox => {
                menu.local.push(SimpleContainer::new(27));
                menu.grid(local, 9, 3, 8, 18, SlotKind::ShulkerBox);
            }
            MenuKind::Hopper => {
                menu.local.push(SimpleContainer::new(5));
                menu.grid(local, 5, 1, 44, 20, SlotKind::Normal);
                inventory_xy.1 = 51;
            }
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace => {
                menu.local.push(SimpleContainer::new(3));
                menu.add_slot(Slot::new(local, 0, 56, 17));
                menu.add_slot(Slot::new(local, 1, 56, 53).with_kind(SlotKind::FurnaceFuel));
                menu.add_slot(
                    Slot::new(local, 2, 116, 35).with_kind(SlotKind::Result(ResultKind::Furnace)),
                );
                menu.add_data(4);
            }
            MenuKind::Crafting => {
                menu.local = vec![SimpleContainer::new(9), SimpleContainer::new(4)];
                menu.add_slot(
                    Slot::new(result, 0, 124, 35).with_kind(SlotKind::Result(ResultKind::Crafting)),
                );
                menu.grid(local, 3, 3, 30, 17, SlotKind::Normal);
            }
            MenuKind::Enchantment => {
                menu.local.push(SimpleContainer::new(2));
                menu.add_slot(Slot::new(local, 0, 15, 47).with_kind(SlotKind::EnchantmentItem));
                menu.add_slot(Slot::new(local, 1, 35, 47).with_kind(SlotKind::Lapis));
                menu.add_data(10);
                menu.data[4..].fill(-1);
            }
            MenuKind::Anvil => {
                menu.local = vec![SimpleContainer::new(2), SimpleContainer::new(4)];
                menu.add_slot(Slot::new(local, 0, 27, 47));
                menu.add_slot(Slot::new(local, 1, 76, 47));
                menu.add_slot(
                    Slot::new(result, 2, 134, 47).with_kind(SlotKind::Result(ResultKind::Anvil)),
                );
                menu.add_data(1);
            }
            MenuKind::Smithing => {
                menu.local = vec![SimpleContainer::new(3), SimpleContainer::new(4)];
                for (i, rule) in [
                    SlotKind::SmithingTemplate,
                    SlotKind::SmithingBase,
                    SlotKind::SmithingAddition,
                ]
                .into_iter()
                .enumerate()
                {
                    menu.add_slot(Slot::new(local, i, 8 + i as i32 * 18, 48).with_kind(rule));
                }
                menu.add_slot(
                    Slot::new(result, 3, 98, 48).with_kind(SlotKind::Result(ResultKind::Smithing)),
                );
                menu.add_data(1);
            }
            MenuKind::Grindstone => {
                menu.local = vec![SimpleContainer::new(2), SimpleContainer::new(4)];
                menu.add_slot(Slot::new(local, 0, 49, 19).with_kind(SlotKind::GrindstoneInput));
                menu.add_slot(Slot::new(local, 1, 49, 40).with_kind(SlotKind::GrindstoneInput));
                menu.add_slot(
                    Slot::new(result, 2, 129, 34)
                        .with_kind(SlotKind::Result(ResultKind::Grindstone)),
                );
            }
            MenuKind::Stonecutter => {
                menu.local = vec![SimpleContainer::new(1), SimpleContainer::new(4)];
                menu.add_slot(Slot::new(local, 0, 20, 33));
                menu.add_slot(
                    Slot::new(result, 1, 143, 33)
                        .with_kind(SlotKind::Result(ResultKind::Stonecutter)),
                );
                menu.add_data(1);
                menu.data[0] = -1;
            }
            MenuKind::Loom => {
                menu.local = vec![SimpleContainer::new(3), SimpleContainer::new(4)];
                for (i, (x, y, rule)) in [
                    (13, 26, SlotKind::Banner),
                    (33, 26, SlotKind::Dye),
                    (23, 45, SlotKind::BannerPattern),
                ]
                .into_iter()
                .enumerate()
                {
                    menu.add_slot(Slot::new(local, i, x, y).with_kind(rule));
                }
                menu.add_slot(
                    Slot::new(result, 0, 143, 57).with_kind(SlotKind::Result(ResultKind::Loom)),
                );
                menu.add_data(1);
                menu.data[0] = -1;
            }
            MenuKind::Cartography => {
                menu.local = vec![SimpleContainer::new(2), SimpleContainer::new(4)];
                menu.add_slot(Slot::new(local, 0, 15, 15).with_kind(SlotKind::Map));
                menu.add_slot(Slot::new(local, 1, 15, 52).with_kind(SlotKind::MapAddition));
                menu.add_slot(
                    Slot::new(result, 2, 145, 39)
                        .with_kind(SlotKind::Result(ResultKind::Cartography)),
                );
            }
            MenuKind::Beacon => {
                menu.local.push(SimpleContainer::new(1));
                menu.add_slot(Slot::new(local, 0, 136, 110).with_kind(SlotKind::BeaconPayment));
                menu.add_data(3);
                inventory_xy = (36, 137);
            }
            MenuKind::BrewingStand => {
                menu.local.push(SimpleContainer::new(5));
                for (i, (x, y)) in [(56, 51), (79, 58), (102, 51)].into_iter().enumerate() {
                    menu.add_slot(Slot::new(local, i, x, y).with_kind(SlotKind::BrewingBottle));
                }
                menu.add_slot(Slot::new(local, 3, 79, 17).with_kind(SlotKind::BrewingIngredient));
                menu.add_slot(Slot::new(local, 4, 17, 17).with_kind(SlotKind::BrewingFuel));
                menu.add_data(4);
            }
            MenuKind::Merchant => {
                menu.local.push(SimpleContainer::new(3));
                menu.add_slot(Slot::new(local, 0, 136, 37));
                menu.add_slot(Slot::new(local, 1, 162, 37));
                menu.add_slot(
                    Slot::new(local, 2, 220, 37).with_kind(SlotKind::Result(ResultKind::Merchant)),
                );
                inventory_xy = (108, 84);
            }
            MenuKind::Lectern => {
                menu.local.push(SimpleContainer::new(1));
                menu.add_slot(Slot::new(local, 0, 0, 0));
                menu.add_data(1);
                return Ok(menu);
            }
            MenuKind::Horse { .. } | MenuKind::Nautilus => {
                let columns = if let MenuKind::Horse { columns } = kind {
                    columns
                } else {
                    0
                };
                menu.local = vec![
                    SimpleContainer::new(usize::from(columns) * 3),
                    SimpleContainer::new(1),
                    SimpleContainer::new(1),
                ];
                menu.add_slot(
                    Slot::new(ContainerId::Local(1), 0, 8, 18)
                        .with_kind(SlotKind::MountEquipment(EquipmentSlot::Saddle)),
                );
                menu.add_slot(
                    Slot::new(ContainerId::Local(2), 0, 8, 36)
                        .with_kind(SlotKind::MountEquipment(EquipmentSlot::Body)),
                );
                menu.grid(local, usize::from(columns), 3, 80, 18, SlotKind::Normal);
            }
            MenuKind::Chest { .. } => {
                return Err(MenuError::Unsupported("chest rows outside 1..=6"));
            }
        }
        menu.standard_inventory(inventory_xy.0, inventory_xy.1);
        if kind == MenuKind::Inventory {
            menu.add_slot(Slot::new(ContainerId::Player, 40, 77, 62).with_kind(SlotKind::Offhand));
        }
        if kind == MenuKind::Crafter {
            menu.add_slot(Slot::new(result, 0, 134, 35).with_kind(SlotKind::NonInteractiveResult));
        }
        Ok(menu)
    }
    fn grid(
        &mut self,
        container: ContainerId,
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        kind: SlotKind,
    ) {
        for row in 0..height {
            for column in 0..width {
                self.add_slot(
                    Slot::new(
                        container,
                        column + row * width,
                        x + column as i32 * 18,
                        y + row as i32 * 18,
                    )
                    .with_kind(kind),
                );
            }
        }
    }
    fn standard_inventory(&mut self, x: i32, y: i32) {
        for row in 0..3 {
            for column in 0..9 {
                self.add_slot(Slot::new(
                    ContainerId::Player,
                    column + (row + 1) * 9,
                    x + column as i32 * 18,
                    y + row as i32 * 18,
                ));
            }
        }
        for column in 0..9 {
            self.add_slot(Slot::new(
                ContainerId::Player,
                column,
                x + column as i32 * 18,
                y + 58,
            ));
        }
    }
    /// Bind a local container's slots to a live block/entity inventory after validating its size.
    pub fn bind_external(
        &mut self,
        local: usize,
        external: usize,
        container: &dyn Container,
    ) -> Result<()> {
        let expected = self.local[local].size();
        if container.size() < expected {
            return Err(MenuError::ContainerTooSmall {
                actual: container.size(),
                expected,
            });
        }
        for slot in &mut self.slots {
            if slot.container == ContainerId::Local(local) {
                slot.container = ContainerId::External(external);
            }
        }
        Ok(())
    }
    /// Port of concrete menus' quickMoveStack section routing.
    pub fn quick_move_stack(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: usize,
        hooks: &mut dyn MenuHooks,
    ) -> Result<ItemStack> {
        let mut stack = self.get_item(ctx, index);
        if stack.is_empty() {
            return Ok(ItemStack::default());
        }
        let original = stack.clone();
        let slots = self.slots.len();
        let moved = match self.kind {
            MenuKind::Chest { rows } => {
                self.move_between(ctx, &mut stack, index, usize::from(rows) * 9, slots, hooks)?
            }
            MenuKind::Dispenser | MenuKind::Crafter => {
                self.move_between(ctx, &mut stack, index, 9, 45, hooks)?
            }
            MenuKind::ShulkerBox => self.move_between(ctx, &mut stack, index, 27, slots, hooks)?,
            MenuKind::Hopper => self.move_between(ctx, &mut stack, index, 5, slots, hooks)?,
            MenuKind::Inventory => {
                if index == 0 {
                    self.move_item_stack_to(ctx, &mut stack, 9, 45, true, hooks)?
                } else if index < 9 {
                    self.move_item_stack_to(ctx, &mut stack, 9, 45, false, hooks)?
                } else if let Some(target) = self.equipment_target(ctx, &stack) {
                    self.move_item_stack_to(ctx, &mut stack, target, target + 1, false, hooks)?
                } else if index < 36 {
                    self.move_item_stack_to(ctx, &mut stack, 36, 45, false, hooks)?
                } else if index < 45 {
                    self.move_item_stack_to(ctx, &mut stack, 9, 36, false, hooks)?
                } else {
                    self.move_item_stack_to(ctx, &mut stack, 9, 45, false, hooks)?
                }
            }
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace => {
                if index == 2 {
                    self.move_item_stack_to(ctx, &mut stack, 3, 39, true, hooks)?
                } else if index < 2 {
                    self.move_item_stack_to(ctx, &mut stack, 3, 39, false, hooks)?
                } else if hooks.can_smelt(&stack)? {
                    self.move_item_stack_to(ctx, &mut stack, 0, 1, false, hooks)?
                } else if component(&stack, ComponentTypeId::COOKING_FUEL).is_some() {
                    self.move_item_stack_to(ctx, &mut stack, 1, 2, false, hooks)?
                } else {
                    self.move_player_sections(ctx, &mut stack, index, 3, hooks)?
                }
            }
            MenuKind::Crafting => {
                if index == 0 {
                    self.move_item_stack_to(ctx, &mut stack, 10, 46, true, hooks)?
                } else if index < 10 {
                    self.move_item_stack_to(ctx, &mut stack, 10, 46, false, hooks)?
                } else {
                    self.move_item_stack_to(ctx, &mut stack, 1, 10, false, hooks)?
                        || self.move_player_sections(ctx, &mut stack, index, 10, hooks)?
                }
            }
            MenuKind::Enchantment => {
                if index < 2 {
                    self.move_item_stack_to(ctx, &mut stack, 2, 38, true, hooks)?
                } else if stack
                    .0
                    .as_ref()
                    .is_some_and(|v| v.item == Item::LAPIS_LAZULI)
                {
                    self.move_item_stack_to(ctx, &mut stack, 1, 2, true, hooks)?
                } else if self.get_item(ctx, 0).is_empty()
                    && self.slots[0].may_place(&stack, hooks)?
                {
                    self.set_item(ctx, 0, split(&mut stack, 1));
                    true
                } else {
                    false
                }
            }
            MenuKind::Merchant => {
                if index == 2 {
                    self.move_item_stack_to(ctx, &mut stack, 3, 39, true, hooks)?
                } else if index < 2 {
                    self.move_item_stack_to(ctx, &mut stack, 3, 39, false, hooks)?
                } else {
                    self.move_player_sections(ctx, &mut stack, index, 3, hooks)?
                }
            }
            MenuKind::Beacon => {
                if index == 0 {
                    self.move_item_stack_to(ctx, &mut stack, 1, 37, true, hooks)?
                } else if self.get_item(ctx, 0).is_empty()
                    && self.slots[0].may_place(&stack, hooks)?
                    && count(&stack) == 1
                {
                    self.move_item_stack_to(ctx, &mut stack, 0, 1, false, hooks)?
                } else {
                    self.move_player_sections(ctx, &mut stack, index, 1, hooks)?
                }
            }
            MenuKind::Anvil | MenuKind::Smithing => {
                let result = if self.kind == MenuKind::Anvil { 2 } else { 3 };
                if index <= result {
                    self.move_item_stack_to(
                        ctx,
                        &mut stack,
                        result + 1,
                        result + 37,
                        index == result,
                        hooks,
                    )?
                } else {
                    let mut accepts = self.kind == MenuKind::Anvil;
                    if !accepts {
                        for i in 0..result {
                            if self.get_item(ctx, i).is_empty()
                                && self.slots[i].may_place(&stack, hooks)?
                            {
                                accepts = true;
                                break;
                            }
                        }
                    }
                    if accepts {
                        self.move_item_stack_to(ctx, &mut stack, 0, result, false, hooks)?
                    } else {
                        self.move_player_sections(ctx, &mut stack, index, result + 1, hooks)?
                    }
                }
            }
            MenuKind::Grindstone => {
                if index <= 2 {
                    self.move_item_stack_to(ctx, &mut stack, 3, 39, index == 2, hooks)?
                } else if !self.get_item(ctx, 0).is_empty() && !self.get_item(ctx, 1).is_empty() {
                    self.move_player_sections(ctx, &mut stack, index, 3, hooks)?
                } else {
                    self.move_item_stack_to(ctx, &mut stack, 0, 2, false, hooks)?
                }
            }
            MenuKind::Stonecutter => {
                if index < 2 {
                    self.move_item_stack_to(ctx, &mut stack, 2, 38, index == 1, hooks)?
                } else if hooks.accepts_stonecutter(&stack)? {
                    self.move_item_stack_to(ctx, &mut stack, 0, 1, false, hooks)?
                } else {
                    self.move_player_sections(ctx, &mut stack, index, 2, hooks)?
                }
            }
            MenuKind::Loom | MenuKind::Cartography => {
                let inputs = if self.kind == MenuKind::Loom { 3 } else { 2 };
                if index <= inputs {
                    self.move_item_stack_to(
                        ctx,
                        &mut stack,
                        inputs + 1,
                        inputs + 37,
                        index == inputs,
                        hooks,
                    )?
                } else {
                    let mut destination = None;
                    for i in 0..inputs {
                        if self.slots[i].may_place(&stack, hooks)? {
                            destination = Some(i);
                            break;
                        }
                    }
                    if let Some(i) = destination {
                        self.move_item_stack_to(ctx, &mut stack, i, i + 1, false, hooks)?
                    } else {
                        self.move_player_sections(ctx, &mut stack, index, inputs + 1, hooks)?
                    }
                }
            }
            MenuKind::BrewingStand => {
                if index < 5 {
                    self.move_item_stack_to(ctx, &mut stack, 5, 41, true, hooks)?
                } else if self.slots[4].may_place(&stack, hooks)? {
                    // Vanilla returns EMPTY immediately after a successful fuel transfer.
                    // Write through the canonical source before that early return.
                    if self.move_item_stack_to(ctx, &mut stack, 4, 5, false, hooks)? {
                        self.set_item(ctx, index, stack);
                        return Ok(ItemStack::default());
                    }
                    if self.slots[3].may_place(&stack, hooks)? {
                        self.move_item_stack_to(ctx, &mut stack, 3, 4, false, hooks)?
                    } else {
                        true
                    }
                } else if self.slots[3].may_place(&stack, hooks)? {
                    self.move_item_stack_to(ctx, &mut stack, 3, 4, false, hooks)?
                } else if self.slots[0].may_place(&stack, hooks)? {
                    self.move_item_stack_to(ctx, &mut stack, 0, 3, false, hooks)?
                } else {
                    self.move_player_sections(ctx, &mut stack, index, 5, hooks)?
                }
            }
            MenuKind::Horse { .. } | MenuKind::Nautilus => {
                let start = self.slots.len() - 36;
                if index < start {
                    self.move_item_stack_to(ctx, &mut stack, start, slots, true, hooks)?
                } else if self.slots[1].may_place(&stack, hooks)?
                    && self.get_item(ctx, 1).is_empty()
                {
                    self.move_item_stack_to(ctx, &mut stack, 1, 2, false, hooks)?
                } else if self.slots[0].may_place(&stack, hooks)?
                    && self.get_item(ctx, 0).is_empty()
                {
                    self.move_item_stack_to(ctx, &mut stack, 0, 1, false, hooks)?
                } else if start > 2
                    && self.move_item_stack_to(ctx, &mut stack, 2, start, false, hooks)?
                {
                    true
                } else {
                    self.move_player_sections(ctx, &mut stack, index, start, hooks)?;
                    self.set_item(ctx, index, stack);
                    return Ok(ItemStack::default());
                }
            }
            MenuKind::Lectern => false,
        };
        if !moved {
            return Ok(ItemStack::default());
        }
        let remaining = count(&stack);
        self.set_item(
            ctx,
            index,
            if stack.is_empty() {
                ItemStack::default()
            } else {
                stack.clone()
            },
        );
        if remaining == count(&original) {
            return Ok(ItemStack::default());
        }
        if !matches!(
            self.kind,
            MenuKind::Chest { .. }
                | MenuKind::Hopper
                | MenuKind::ShulkerBox
                | MenuKind::Horse { .. }
                | MenuKind::Nautilus
        ) {
            let slot = self.slots[index].clone();
            let crafted = count(&original) - remaining;
            // Recipe hooks are checked before a result is taken through clicked().
            // Direct quickMoveStack callers must provide the same result hook.
            self.take_hook(ctx, &slot, &stack, crafted, hooks)?;
        }
        if matches!(self.kind, MenuKind::Inventory | MenuKind::Crafting) && index == 0
            || self.kind == MenuKind::Stonecutter && index == 1
        {
            if !stack.is_empty() {
                ctx.drops.push(stack);
            }
        }
        Ok(original)
    }
    fn take_hook(
        &mut self,
        ctx: &mut MenuContext<'_>,
        slot: &Slot,
        stack: &ItemStack,
        crafted: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        match slot.container {
            ContainerId::Local(i) => slot.on_take(&mut self.local[i], stack, crafted, hooks),
            ContainerId::Player => slot.on_take(&mut ctx.player, stack, crafted, hooks),
            ContainerId::External(i) => slot.on_take(&mut *ctx.external[i], stack, crafted, hooks),
        }
    }
    fn move_between(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: &mut ItemStack,
        index: usize,
        boundary: usize,
        end: usize,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        if index < boundary {
            self.move_item_stack_to(ctx, stack, boundary, end, true, hooks)
        } else {
            self.move_item_stack_to(ctx, stack, 0, boundary, false, hooks)
        }
    }
    fn move_player_sections(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: &mut ItemStack,
        index: usize,
        start: usize,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        if index < start + 27 {
            self.move_item_stack_to(ctx, stack, start + 27, start + 36, false, hooks)
        } else {
            self.move_item_stack_to(ctx, stack, start, start + 27, false, hooks)
        }
    }
    fn equipment_target(&self, ctx: &MenuContext<'_>, stack: &ItemStack) -> Option<usize> {
        let equipment = match component(stack, ComponentTypeId::EQUIPPABLE) {
            Some(DataComponent::Equippable(v)) => v.slot,
            _ => EquipmentSlot::Mainhand,
        };
        let target = match equipment {
            EquipmentSlot::Feet => 8,
            EquipmentSlot::Legs => 7,
            EquipmentSlot::Chest => 6,
            EquipmentSlot::Head => 5,
            EquipmentSlot::Offhand => 45,
            _ => return None,
        };
        self.get_item(ctx, target).is_empty().then_some(target)
    }
}

/// Port of ServerPlayer's inventoryMenu/containerMenu fields and container counter.
#[derive(Debug)]
pub struct MenuSession {
    pub inventory_menu: AbstractContainerMenu,
    pub container_menu: Option<AbstractContainerMenu>,
    pub container_counter: i32,
    creative_drop_count: u64,
    creative_drop_tick: u64,
}
impl Default for MenuSession {
    fn default() -> Self {
        Self {
            inventory_menu: AbstractContainerMenu::new(MenuKind::Inventory, 0)
                .expect("player menu"),
            container_menu: None,
            container_counter: 0,
            creative_drop_count: 0,
            creative_drop_tick: 0,
        }
    }
}
impl MenuSession {
    pub fn current(&self) -> &AbstractContainerMenu {
        self.container_menu.as_ref().unwrap_or(&self.inventory_menu)
    }
    pub fn current_mut(&mut self) -> &mut AbstractContainerMenu {
        self.container_menu
            .as_mut()
            .unwrap_or(&mut self.inventory_menu)
    }
    pub fn next_container_counter(&mut self) -> i32 {
        self.container_counter = self.container_counter % 100 + 1;
        self.container_counter
    }
}

/// Port of MenuProvider. Block behavior calls this at its synchronous openMenu site.
pub trait MenuProvider {
    fn display_name(&self) -> ferraloom_protocol::chat::Component;
    fn create_menu(
        &self,
        id: i32,
        player: crate::entity::EntityRef,
        level: &mut crate::level::Level,
    ) -> Result<Option<AbstractContainerMenu>>;
}

impl AbstractContainerMenu {
    /// Port of AbstractContainerMenu#removed and concrete menus' input cleanup.
    /// Slot updates and drops remain in vanilla call order for the runtime adapter.
    pub fn removed(
        &mut self,
        ctx: &mut MenuContext<'_>,
        disconnected: bool,
    ) -> Vec<crate::entity::player::inventory::InventoryEffect> {
        use crate::entity::player::inventory::InventoryEffect;
        let mut effects = Vec::new();
        let mut carried = std::mem::take(&mut self.carried);
        if !carried.is_empty() {
            if disconnected {
                effects.push(InventoryEffect::Drop(carried));
            } else {
                effects.extend(ctx.player.inventory.place_item_back_in_inventory(
                    &mut carried,
                    true,
                    ctx.player.equipment,
                    ctx.creative,
                ));
            }
        }
        if self.kind == MenuKind::Beacon {
            let slot = self.slots[0].clone();
            let mut payment = self.get_item(ctx, 0);
            let amount = count(&payment).min(1);
            let dropped = split(&mut payment, amount);
            self.set_item(ctx, slot.index, payment);
            if !dropped.is_empty() {
                effects.push(InventoryEffect::Drop(dropped));
            }
            return effects;
        }
        let input_count = match self.kind {
            MenuKind::Inventory => 4,
            MenuKind::Crafting => 9,
            MenuKind::Enchantment
            | MenuKind::Anvil
            | MenuKind::Grindstone
            | MenuKind::Cartography
            | MenuKind::Merchant => 2,
            MenuKind::Loom | MenuKind::Smithing => 3,
            MenuKind::Stonecutter => 1,
            _ => 0,
        };
        for index in 0..input_count {
            let mut stack = self.local[0].remove_item_no_update(index);
            if !stack.is_empty() {
                if disconnected {
                    effects.push(InventoryEffect::Drop(stack));
                } else {
                    effects.extend(ctx.player.inventory.place_item_back_in_inventory(
                        &mut stack,
                        true,
                        ctx.player.equipment,
                        ctx.creative,
                    ));
                }
            }
        }
        if self.kind == MenuKind::Inventory {
            self.local[1].clear_content();
        }
        for container in &mut *ctx.external {
            container.stop_open();
        }
        effects
    }
}
impl MenuSession {
    /// Port of ServerPlayer#doCloseContainer. A client close ignores its supplied ID.
    pub fn do_close_container(
        &mut self,
        ctx: &mut MenuContext<'_>,
        disconnected: bool,
    ) -> Vec<crate::entity::player::inventory::InventoryEffect> {
        if let Some(menu) = &mut self.container_menu {
            let effects = menu.removed(ctx, disconnected);
            self.inventory_menu.transfer_state(menu);
            self.container_menu = None;
            effects
        } else {
            self.inventory_menu.removed(ctx, disconnected)
        }
    }
    /// Port of ServerGamePacketListenerImpl#tryPickItem, after the item's clone query.
    pub fn try_pick_item(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: ItemStack,
    ) -> Result<Vec<ferraloom_protocol::play::clientbound::Packet>> {
        use ferraloom_protocol::{codec::VarInt, play::clientbound as cb};
        let inventory = &mut ctx.player.inventory;
        if let Some(slot) = inventory.find_slot_matching_item(&stack) {
            if slot < 9 {
                inventory.set_selected_slot(slot);
            } else {
                inventory.pick_slot(slot);
            }
        } else if ctx.creative {
            inventory.add_and_pick_item(stack);
        }
        let mut packets = vec![cb::Packet::SetHeldSlot(cb::SetHeldSlot {
            slot: VarInt(inventory.selected_slot() as i32),
        })];
        self.inventory_menu.broadcast_changes(ctx)?;
        packets.extend(self.inventory_menu.drain_packets());
        Ok(packets)
    }
}

impl MenuSession {
    /// Port of TickThrottler(20, 1480), decayed at the connection's doTick count.
    pub fn allow_creative_drop(&mut self, connection_tick: u64) -> bool {
        self.creative_drop_count = self
            .creative_drop_count
            .saturating_sub(connection_tick.saturating_sub(self.creative_drop_tick));
        self.creative_drop_tick = connection_tick;
        if self.creative_drop_count < 1480 {
            self.creative_drop_count += 20;
            true
        } else {
            false
        }
    }
}
