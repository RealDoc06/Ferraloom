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
                menu.local = vec![MenuContainer::crafting(4), MenuContainer::simple(4)];
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
                menu.local
                    .push(MenuContainer::simple(usize::from(rows) * 9));
                menu.grid(local, 9, usize::from(rows), 8, 18, SlotKind::Normal);
                inventory_xy.1 = 18 + i32::from(rows) * 18 + 13;
            }
            MenuKind::Dispenser | MenuKind::Crafter => {
                menu.local = vec![MenuContainer::simple(9), MenuContainer::simple(4)];
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
                menu.local.push(MenuContainer::simple(27));
                menu.grid(local, 9, 3, 8, 18, SlotKind::ShulkerBox);
            }
            MenuKind::Hopper => {
                menu.local.push(MenuContainer::simple(5));
                menu.grid(local, 5, 1, 44, 20, SlotKind::Normal);
                inventory_xy.1 = 51;
            }
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace => {
                menu.local.push(MenuContainer::simple(3));
                menu.add_slot(Slot::new(local, 0, 56, 17));
                menu.add_slot(Slot::new(local, 1, 56, 53).with_kind(SlotKind::FurnaceFuel));
                menu.add_slot(
                    Slot::new(local, 2, 116, 35).with_kind(SlotKind::Result(ResultKind::Furnace)),
                );
                menu.add_data(4);
            }
            MenuKind::Crafting => {
                menu.local = vec![MenuContainer::crafting(9), MenuContainer::simple(4)];
                menu.add_slot(
                    Slot::new(result, 0, 124, 35).with_kind(SlotKind::Result(ResultKind::Crafting)),
                );
                menu.grid(local, 3, 3, 30, 17, SlotKind::Normal);
            }
            MenuKind::Enchantment => {
                menu.local.push(MenuContainer::simple(2));
                menu.add_slot(Slot::new(local, 0, 15, 47).with_kind(SlotKind::EnchantmentItem));
                menu.add_slot(Slot::new(local, 1, 35, 47).with_kind(SlotKind::Lapis));
                menu.add_data(10);
                menu.data[4..].fill(-1);
            }
            MenuKind::Anvil => {
                menu.local = vec![MenuContainer::simple(2), MenuContainer::simple(4)];
                menu.add_slot(Slot::new(local, 0, 27, 47));
                menu.add_slot(Slot::new(local, 1, 76, 47));
                menu.add_slot(
                    Slot::new(result, 2, 134, 47).with_kind(SlotKind::Result(ResultKind::Anvil)),
                );
                menu.add_data(1);
            }
            MenuKind::Smithing => {
                menu.local = vec![MenuContainer::simple(3), MenuContainer::simple(4)];
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
                menu.local = vec![MenuContainer::simple(2), MenuContainer::simple(4)];
                menu.add_slot(Slot::new(local, 0, 49, 19).with_kind(SlotKind::GrindstoneInput));
                menu.add_slot(Slot::new(local, 1, 49, 40).with_kind(SlotKind::GrindstoneInput));
                menu.add_slot(
                    Slot::new(result, 2, 129, 34)
                        .with_kind(SlotKind::Result(ResultKind::Grindstone)),
                );
            }
            MenuKind::Stonecutter => {
                menu.local = vec![MenuContainer::simple(1), MenuContainer::simple(4)];
                menu.add_slot(Slot::new(local, 0, 20, 33));
                menu.add_slot(
                    Slot::new(result, 1, 143, 33)
                        .with_kind(SlotKind::Result(ResultKind::Stonecutter)),
                );
                menu.add_data(1);
            }
            MenuKind::Loom => {
                menu.local = vec![MenuContainer::simple(3), MenuContainer::simple(4)];
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
                menu.local = vec![MenuContainer::simple(2), MenuContainer::simple(4)];
                menu.add_slot(Slot::new(local, 0, 15, 15).with_kind(SlotKind::Map));
                menu.add_slot(Slot::new(local, 1, 15, 52).with_kind(SlotKind::MapAddition));
                menu.add_slot(
                    Slot::new(result, 2, 145, 39)
                        .with_kind(SlotKind::Result(ResultKind::Cartography)),
                );
            }
            MenuKind::Beacon => {
                menu.local.push(MenuContainer::simple(1));
                menu.add_slot(Slot::new(local, 0, 136, 110).with_kind(SlotKind::BeaconPayment));
                menu.add_data(3);
                inventory_xy = (36, 137);
            }
            MenuKind::BrewingStand => {
                menu.local.push(MenuContainer::simple(5));
                for (i, (x, y)) in [(56, 51), (79, 58), (102, 51)].into_iter().enumerate() {
                    menu.add_slot(Slot::new(local, i, x, y).with_kind(SlotKind::BrewingBottle));
                }
                menu.add_slot(Slot::new(local, 3, 79, 17).with_kind(SlotKind::BrewingIngredient));
                menu.add_slot(Slot::new(local, 4, 17, 17).with_kind(SlotKind::BrewingFuel));
                menu.add_data(4);
            }
            MenuKind::Merchant => {
                menu.local.push(MenuContainer::simple(3));
                menu.add_slot(Slot::new(local, 0, 136, 37));
                menu.add_slot(Slot::new(local, 1, 162, 37));
                menu.add_slot(
                    Slot::new(local, 2, 220, 37).with_kind(SlotKind::Result(ResultKind::Merchant)),
                );
                inventory_xy = (108, 84);
            }
            MenuKind::Lectern => {
                menu.local.push(MenuContainer::simple(1));
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
                    MenuContainer::simple(usize::from(columns) * 3),
                    MenuContainer::simple(1),
                    MenuContainer::simple(1),
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
        if matches!(
            kind,
            MenuKind::Inventory
                | MenuKind::Crafting
                | MenuKind::Crafter
                | MenuKind::Anvil
                | MenuKind::Grindstone
                | MenuKind::Smithing
                | MenuKind::Stonecutter
                | MenuKind::Loom
                | MenuKind::Cartography
        ) {
            menu.local[1] = MenuContainer::Result(ResultContainer::default());
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
        if matches!(
            self.kind,
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace
        ) {
            let mut operation = super::in_memory::InMemoryMenuOperation::new(self, ctx);
            let stack = super::quick_move_furnace::quick_move_furnace_menu_item(
                &mut operation,
                index,
                hooks,
            )?;
            return operation.read(stack).cloned();
        }
        if matches!(self.kind, MenuKind::Inventory | MenuKind::Crafting) {
            let mut operation = super::in_memory::InMemoryMenuOperation::new(self, ctx);
            let stack = super::quick_move_crafting::quick_move_crafting_menu_item(
                &mut operation,
                index,
                hooks,
            )?;
            return operation.read(stack).cloned();
        }
        if matches!(
            self.kind,
            MenuKind::Chest { .. }
                | MenuKind::Hopper
                | MenuKind::ShulkerBox
                | MenuKind::Dispenser
                | MenuKind::Crafter
        ) {
            return quick_move_container_menu_item(
                &mut super::in_memory::InMemoryMenuOperation::new(self, ctx),
                index,
                hooks,
            );
        }
        let mut stack = self.get_item(ctx, index);
        if stack.is_empty() {
            return Ok(ItemStack::default());
        }
        if let SlotKind::Result(kind) = self.slots[index].kind
            && !hooks.result_available(kind, &stack)?
        {
            return Ok(ItemStack::default());
        }
        if index == 0
            && self.crafting_grid_size().is_some()
            && let Some(crafting) = hooks.crafting_ref()
        {
            crafting.validate_take_result()?;
        }
        let original = stack.clone();
        let slots = self.slots.len();
        let moved = match self.kind {
            MenuKind::Crafter => unreachable!("crafter uses the retained quick-move kernel"),
            MenuKind::Chest { .. }
            | MenuKind::Hopper
            | MenuKind::ShulkerBox
            | MenuKind::Dispenser => {
                unreachable!("simple containers use the shared retained quick-move kernel")
            }
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
                    if let Some(crafting) = hooks.crafting() {
                        crafting.item_crafted(self, ctx, &stack, None)?;
                    }
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
                    self.set_item_live(ctx, 0, split(&mut stack, 1), hooks)?;
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
                        self.set_item_live(ctx, index, stack, hooks)?;
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
                    self.set_item_live(ctx, index, stack, hooks)?;
                    return Ok(ItemStack::default());
                }
            }
            MenuKind::Lectern => false,
        };
        if !moved {
            return Ok(ItemStack::default());
        }
        let remaining = count(&stack);
        if index == 0 && self.crafting_grid_size().is_some() && hooks.has_crafting() {
            self.local[1].set_item(0, stack.clone());
            self.crafting_remove_count = self
                .crafting_remove_count
                .wrapping_add(count(&original) - remaining);
            self.crafting_check_achievements(ctx, &stack, hooks)?;
        }
        if stack.is_empty() {
            self.set_item_live(ctx, index, ItemStack::default(), hooks)?;
        } else {
            // ItemStack mutations affect the canonical stack without Slot#set.
            let slot = self.slots[index].clone();
            self.container_mut(ctx, slot.container)
                .set_item(slot.container_slot, stack.clone());
            if self.crafting_grid_size().is_none() || slot.container != ContainerId::Local(0) {
                self.container_mut(ctx, slot.container).set_changed();
            }
        }
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
            self.on_take(
                ctx,
                slot.index,
                &stack,
                if slot.kind == SlotKind::Result(ResultKind::Crafting) && hooks.has_crafting() {
                    0
                } else {
                    crafted
                },
                hooks,
            )?;
        }
        if ((matches!(self.kind, MenuKind::Inventory | MenuKind::Crafting) && index == 0)
            || (self.kind == MenuKind::Stonecutter && index == 1))
            && !stack.is_empty()
        {
            ctx.drops.push(DroppedStack {
                stack,
                thrown_from_hand: false,
            });
        }
        Ok(original)
    }
    fn move_player_sections(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: &mut ItemStack,
        index: usize,
        start: usize,
        hooks: &mut dyn MenuHooks,
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
/// Generation distinguishes receiver identity when vanilla reuses container IDs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MenuRef {
    pub container_id: i32,
    pub generation: u64,
}

#[derive(Debug)]
pub struct MenuSession {
    pub inventory_menu: AbstractContainerMenu,
    pub container_menu: Option<AbstractContainerMenu>,
    pub container_counter: i32,
    pub pending_packets: Vec<ferraloom_protocol::play::clientbound::Packet>,
    pub(crate) listener_copies: ListenerCopyArena,
    pub(crate) operation_stacks: super::operation_stacks::OperationStackArena,
    detached: std::collections::HashMap<MenuRef, AbstractContainerMenu>,
    retained: std::collections::HashMap<MenuRef, usize>,
    next_generation: u64,
    creative_drop_count: u64,
    creative_drop_tick: u64,
}
impl Default for MenuSession {
    fn default() -> Self {
        let mut inventory_menu =
            AbstractContainerMenu::new(MenuKind::Inventory, 0).expect("player menu");
        inventory_menu.generation = 1;
        Self {
            inventory_menu,
            container_menu: None,
            container_counter: 0,
            pending_packets: Vec::new(),
            listener_copies: ListenerCopyArena::default(),
            operation_stacks: super::operation_stacks::OperationStackArena::default(),
            detached: std::collections::HashMap::new(),
            retained: std::collections::HashMap::new(),
            next_generation: 2,
            creative_drop_count: 0,
            creative_drop_tick: 0,
        }
    }
}
impl MenuSession {
    pub(crate) fn menu_and_listener_copies_mut(
        &mut self,
        reference: MenuRef,
    ) -> Result<(&mut AbstractContainerMenu, &mut ListenerCopyArena)> {
        let menu = if self.inventory_menu.reference() == reference {
            &mut self.inventory_menu
        } else if self
            .container_menu
            .as_ref()
            .is_some_and(|menu| menu.reference() == reference)
        {
            self.container_menu.as_mut().unwrap()
        } else {
            self.detached
                .get_mut(&reference)
                .ok_or(MenuError::MissingMenu(reference))?
        };
        Ok((menu, &mut self.listener_copies))
    }
    pub(crate) fn bind_listener_copies(&mut self, reference: MenuRef) -> Result<()> {
        let (menu, arena) = self.menu_and_listener_copies_mut(reference)?;
        if let Some(standalone) = menu.standalone_listener_copies.take() {
            arena.import(standalone, &mut menu.last_slots);
        }
        Ok(())
    }
    fn retire_listener_copies(&mut self, menu: &AbstractContainerMenu) {
        if menu.standalone_listener_copies.is_none() {
            for &reference in menu.last_slots.iter().flatten() {
                self.listener_copies.release(reference);
            }
        }
    }
    /// Port of doCloseContainer's inventoryMenu.transferState(containerMenu).
    /// Only the player Inventory is shared with inventoryMenu; each menu's local
    /// crafting/result containers have distinct native identities.
    pub(crate) fn transfer_to_inventory_menu(&mut self, source: MenuRef) -> Result<()> {
        let target = self.inventory_menu.reference();
        if source == target {
            return Ok(());
        }
        self.bind_listener_copies(source)?;
        self.bind_listener_copies(target)?;
        let source = self.menu(source)?;
        let mut updates = Vec::new();
        for (index, target_slot) in self.inventory_menu.slots.iter().enumerate() {
            if target_slot.container != ContainerId::Player {
                continue;
            }
            if let Some((source_index, _)) =
                source.slots.iter().enumerate().rev().find(|(_, slot)| {
                    slot.container == ContainerId::Player
                        && slot.container_slot == target_slot.container_slot
                })
            {
                updates.push((
                    index,
                    source.last_slots[source_index],
                    (source.synchronized && self.inventory_menu.synchronized)
                        .then(|| source.remote_slots[source_index].clone()),
                ));
            }
        }
        for (index, new, remote) in updates {
            if let Some(new) = new {
                assert!(self.listener_copies.retain(new));
            }
            if let Some(old) = std::mem::replace(&mut self.inventory_menu.last_slots[index], new) {
                self.listener_copies.release(old);
            }
            if let Some(remote) = remote {
                self.inventory_menu.remote_slots[index] = remote;
            }
        }
        Ok(())
    }
    pub fn current_ref(&self) -> MenuRef {
        self.current().reference()
    }
    pub fn menu(&self, reference: MenuRef) -> Result<&AbstractContainerMenu> {
        if self.inventory_menu.reference() == reference {
            return Ok(&self.inventory_menu);
        }
        if let Some(menu) = self
            .container_menu
            .as_ref()
            .filter(|menu| menu.reference() == reference)
        {
            return Ok(menu);
        }
        self.detached
            .get(&reference)
            .ok_or(MenuError::MissingMenu(reference))
    }
    pub fn menu_mut(&mut self, reference: MenuRef) -> Result<&mut AbstractContainerMenu> {
        if self.inventory_menu.reference() == reference {
            return Ok(&mut self.inventory_menu);
        }
        if self
            .container_menu
            .as_ref()
            .is_some_and(|menu| menu.reference() == reference)
        {
            return Ok(self.container_menu.as_mut().unwrap());
        }
        self.detached
            .get_mut(&reference)
            .ok_or(MenuError::MissingMenu(reference))
    }
    /// Root the exact receiver across an in-flight menu operation and callbacks.
    pub fn retain_menu(&mut self, reference: MenuRef) -> Result<()> {
        self.menu(reference)?;
        let count = self.retained.entry(reference).or_default();
        *count = count.checked_add(1).expect("menu roots exhausted");
        Ok(())
    }
    pub fn release_menu(&mut self, reference: MenuRef) {
        if let Some(count) = self.retained.get_mut(&reference) {
            *count -= 1;
            if *count == 0 {
                self.retained.remove(&reference);
                if let Some(menu) = self.detached.remove(&reference) {
                    self.retire_listener_copies(&menu);
                }
            }
        }
    }
    /// Store and root a newly constructed receiver before initMenu callbacks.
    /// It is addressable by generation while containerMenu still names the old menu.
    pub fn register_container_menu(&mut self, mut menu: AbstractContainerMenu) -> MenuRef {
        menu.generation = self.next_generation;
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .expect("menu generations exhausted");
        let reference = menu.reference();
        self.detached.insert(reference, menu);
        self.retained.insert(reference, 1);
        reference
    }
    /// Port of ServerPlayer#openMenu's assignment after initMenu. Return the
    /// displaced receiver's block roots for the level adapter to release.
    pub fn publish_container_menu(
        &mut self,
        reference: MenuRef,
    ) -> Result<Vec<crate::block::entity::BlockEntityRef>> {
        let menu = self
            .detached
            .remove(&reference)
            .ok_or(MenuError::MissingMenu(reference))?;
        let mut displaced = Vec::new();
        if let Some(old) = self.container_menu.replace(menu) {
            displaced.clone_from(&old.block_entities);
            let reference = old.reference();
            if self.retained.contains_key(&reference) {
                self.detached.insert(reference, old);
            } else {
                self.retire_listener_copies(&old);
            }
        }
        Ok(displaced)
    }
    /// In-memory lifecycle adapter; the live path initializes the registered
    /// receiver before publishing it. No callbacks occur during either move.
    pub fn install_container_menu(&mut self, menu: AbstractContainerMenu) -> MenuRef {
        let reference = self.register_container_menu(menu);
        self.publish_container_menu(reference)
            .expect("newly registered menu");
        self.release_menu(reference);
        reference
    }
    pub fn detach_menu(&mut self, reference: MenuRef) {
        if self
            .container_menu
            .as_ref()
            .is_some_and(|menu| menu.reference() == reference)
        {
            let old = self.container_menu.take().unwrap();
            if self.retained.contains_key(&reference) {
                self.detached.insert(reference, old);
            } else {
                self.retire_listener_copies(&old);
            }
        }
    }
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

impl AbstractContainerMenu {
    pub fn reference(&self) -> MenuRef {
        MenuRef {
            container_id: self.container_id,
            generation: self.generation,
        }
    }
}

/// Port of MenuProvider. Block behavior calls this at its synchronous openMenu site.
pub trait MenuProvider {
    fn block_entity_roots(&self) -> &[crate::block::entity::BlockEntityRef] {
        &[]
    }
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
        if self.kind.is_workstation() {
            return super::workstation_removed::removed_pure(self, ctx, disconnected);
        }
        use crate::entity::player::inventory::InventoryEffect;
        let mut effects = Vec::new();
        let mut carried = self.take_carried_unrooted();
        if !carried.is_empty() {
            if disconnected {
                effects.push(InventoryEffect::Drop(carried));
            } else {
                effects.extend(ctx.player.inventory.place_item_back_in_inventory(
                    &mut carried,
                    true,
                    ctx.player.equipment,
                    ctx.infinite_materials,
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
                        ctx.infinite_materials,
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
            let reference = menu.reference();
            let effects = menu.removed(ctx, disconnected);
            self.transfer_to_inventory_menu(reference)
                .expect("current menu transfer");
            self.detach_menu(reference);
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
        } else if ctx.infinite_materials {
            inventory.add_and_pick_item(stack);
        }
        let mut packets = vec![cb::Packet::SetHeldSlot(cb::SetHeldSlot {
            slot: VarInt(inventory.selected_slot() as i32),
        })];
        let reference = self.inventory_menu.reference();
        self.bind_listener_copies(reference)?;
        let (menu, copies) = self.menu_and_listener_copies_mut(reference)?;
        super::operation::broadcast_menu_changes(&mut super::operation::PureMenuSynchronization {
            menu,
            context: ctx,
            copies: Some(copies),
        })?;
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

impl AbstractContainerMenu {
    /// Port of concrete menus' clickMenuButton overrides. Menus without buttons
    /// return false, even for unexpected client button IDs.
    pub fn click_menu_button(
        &mut self,
        ctx: &mut MenuContext<'_>,
        button: i32,
        may_build: bool,
        hooks: &mut dyn MenuHooks,
    ) -> Result<bool> {
        match self.kind {
            MenuKind::Lectern => {
                match button {
                    100.. => self.data[0] = button - 100,
                    1 => self.data[0] = self.data[0].wrapping_sub(1),
                    2 => self.data[0] = self.data[0].wrapping_add(1),
                    3 if may_build => {
                        let slot = self.slots[0].clone();
                        let container: &mut dyn Container = match slot.container {
                            ContainerId::Local(i) => &mut self.local[i],
                            ContainerId::Player => &mut ctx.player,
                            ContainerId::External(i) => &mut *ctx.external[i],
                        };
                        let mut book = container.remove_item_no_update(slot.container_slot);
                        container.set_changed();
                        if !ctx.player.inventory.add(
                            &mut book,
                            ctx.player.equipment,
                            ctx.infinite_materials,
                        ) && !book.is_empty()
                        {
                            ctx.drops.push(DroppedStack {
                                stack: book,
                                thrown_from_hand: false,
                            });
                        }
                    }
                    _ => return Ok(false),
                }
                Ok(true)
            }
            MenuKind::Enchantment => {
                if !(0..3).contains(&button)
                    || self.data[button as usize] <= 0
                    || self.get_item(ctx, 0).is_empty()
                    || !ctx.infinite_materials && count(&self.get_item(ctx, 1)) < button + 1
                {
                    Ok(false)
                } else {
                    hooks.click_button(button)
                }
            }
            MenuKind::Stonecutter => {
                if self.data[0] == button {
                    Ok(false)
                } else if self.get_item(ctx, 0).is_empty() {
                    Ok(true)
                } else {
                    hooks.click_button(button)
                }
            }
            MenuKind::Loom => {
                if button < 0
                    || self.get_item(ctx, 0).is_empty()
                    || self.get_item(ctx, 1).is_empty()
                {
                    Ok(false)
                } else {
                    hooks.click_button(button)
                }
            }
            _ => Ok(false),
        }
    }
    /// Port of CrafterMenu#setSlotState; the block-entity owner invokes this after
    /// updating its canonical ContainerData. Disabled slots still allow extraction.
    pub fn set_crafter_slot_state(
        &mut self,
        ctx: &MenuContext<'_>,
        slot: usize,
        enabled: bool,
    ) -> Result<()> {
        if self.kind != MenuKind::Crafter || slot >= 9 {
            return Err(MenuError::InvalidSlot(slot as i32));
        }
        self.data[slot] = i32::from(!enabled);
        self.slots[slot].disabled = !enabled;
        self.broadcast_changes(ctx)
    }
}
