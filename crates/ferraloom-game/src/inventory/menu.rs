use super::*;
use ferraloom_protocol::{component::HashedStack, play::ContainerInput};

/// Port of the stack and thrown-from-hand argument at a LivingEntity#drop call.
#[derive(Clone, Debug)]
pub struct DroppedStack {
    pub stack: ItemStack,
    pub thrown_from_hand: bool,
}

/// Standalone storage for the legacy menu adapters. Runtime operations resolve
/// their handles through Level rather than retaining these field borrows.
pub struct MenuContext<'a> {
    pub(super) in_memory_stacks: super::in_memory::InMemoryStackState,
    pub player: PlayerContainer<'a>,
    pub external: &'a mut [&'a mut dyn Container],
    pub is_creative: bool,
    pub infinite_materials: bool,
    pub can_drop: bool,
    pub drops: Vec<DroppedStack>,
}
impl<'a> MenuContext<'a> {
    pub fn player(
        inventory: &'a mut crate::entity::player::inventory::Inventory,
        equipment: &'a mut crate::entity::player::equipment::EntityEquipment,
        creative: bool,
    ) -> Self {
        Self::player_with_abilities(inventory, equipment, creative, creative)
    }
    pub fn player_with_abilities(
        inventory: &'a mut crate::entity::player::inventory::Inventory,
        equipment: &'a mut crate::entity::player::equipment::EntityEquipment,
        is_creative: bool,
        infinite_materials: bool,
    ) -> Self {
        Self {
            in_memory_stacks: Default::default(),
            player: PlayerContainer {
                inventory,
                equipment,
            },
            external: &mut [],
            is_creative,
            infinite_materials,
            can_drop: true,
            drops: Vec::new(),
        }
    }
}

/// Port of AbstractContainerMenu. Slots address live containers; only remote and
/// listener snapshots are copied. Clicks themselves never increment the state ID.
#[derive(Debug)]
pub struct AbstractContainerMenu {
    pub(crate) generation: u64,
    pub kind: MenuKind,
    pub container_id: i32,
    pub slots: Vec<Slot>,
    pub local: Vec<MenuContainer>,
    pub block_entities: Vec<crate::block::entity::BlockEntityRef>,
    pub level_access: Option<ContainerLevelAccess>,
    pub data: Vec<i32>,
    pub carried: ItemStack,
    pub(crate) carried_generations: StackGenerations,
    state_id: i32,
    pub(super) quickcraft_type: i32,
    pub(super) quickcraft_status: i32,
    // JVM Slot identity hashes have non-parity order; use deterministic insertion order.
    pub(super) quickcraft_slots: Vec<usize>,
    pub(super) quickcraft_mod_count: i32,
    pub(super) last_slots: Vec<Option<ListenerCopyRef>>,
    pub(super) standalone_listener_copies: Option<ListenerCopyArena>,
    pub(super) player_listener: bool,
    pub(super) crafter_listener: bool,
    pub(super) last_data: Vec<i32>,
    pub(super) remote_slots: Vec<RemoteSlot>,
    pub(super) remote_carried: RemoteSlot,
    pub(super) remote_data: Vec<i32>,
    pub(super) synchronized: bool,
    pub(super) suppressed: bool,
    pub changes: Vec<ContainerChange>,
    pub updates: Vec<ContainerUpdate>,
    pub(crate) crafting_recipe_used: Option<ferraloom_data::datapack::Identifier>,
    pub(crate) crafting_remove_count: i32,
    pub(crate) furnace_remove_count: i32,
    pub(crate) placing_recipe: bool,
}
impl AbstractContainerMenu {
    pub(crate) fn empty(kind: MenuKind, container_id: i32) -> Self {
        Self {
            generation: 0,
            kind,
            container_id,
            slots: Vec::new(),
            local: Vec::new(),
            block_entities: Vec::new(),
            level_access: None,
            data: Vec::new(),
            carried: ItemStack::default(),
            carried_generations: StackGenerations::new(1),
            state_id: 0,
            quickcraft_type: -1,
            quickcraft_status: 0,
            quickcraft_slots: Vec::new(),
            quickcraft_mod_count: 0,
            last_slots: Vec::new(),
            standalone_listener_copies: Some(ListenerCopyArena::default()),
            player_listener: false,
            crafter_listener: false,
            last_data: Vec::new(),
            remote_slots: Vec::new(),
            remote_carried: RemoteSlot::Placeholder,
            remote_data: Vec::new(),
            synchronized: false,
            suppressed: false,
            changes: Vec::new(),
            updates: Vec::new(),
            crafting_recipe_used: None,
            crafting_remove_count: 0,
            furnace_remove_count: 0,
            placing_recipe: false,
        }
    }
    /// Port of AbstractContainerMenu#setCarried. Count edits use the existing
    /// receiver; replacing it detaches any operation-rooted cursor stack.
    pub fn set_carried(&mut self, stack: ItemStack) {
        self.carried_generations
            .replace(0, &mut self.carried, stack);
    }
    pub(crate) fn take_carried_unrooted(&mut self) -> ItemStack {
        self.carried_generations
            .replace(0, &mut self.carried, ItemStack::default())
            .into_owned()
            .expect("live cursor removal uses StackRef")
    }
    pub(crate) fn add_slot(&mut self, mut slot: Slot) {
        slot.index = self.slots.len();
        self.slots.push(slot);
        self.last_slots.push(None);
        self.remote_slots.push(if self.synchronized {
            RemoteSlot::Unknown
        } else {
            RemoteSlot::Placeholder
        });
    }
    pub(crate) fn add_data(&mut self, count: usize) {
        self.data.resize(count, 0);
        self.last_data.resize(count, 0);
        self.remote_data.resize(count, 0);
    }
    /// Port of AbstractContainerMenu#setItem/initializeContents on the receiving side.
    pub fn initialize_contents(
        &mut self,
        ctx: &mut MenuContext<'_>,
        state_id: i32,
        items: &[ItemStack],
        carried: ItemStack,
    ) {
        for (i, stack) in items.iter().enumerate() {
            self.set_item(ctx, i, stack.clone());
        }
        self.set_carried(carried);
        self.state_id = state_id;
    }
    pub fn set_item_with_state(
        &mut self,
        ctx: &mut MenuContext<'_>,
        slot: usize,
        state_id: i32,
        stack: ItemStack,
    ) {
        self.set_item(ctx, slot, stack);
        self.state_id = state_id;
    }
    pub fn find_slot(&self, container: ContainerId, container_slot: usize) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.container == container && s.container_slot == container_slot)
    }
    pub fn state_id(&self) -> i32 {
        self.state_id
    }
    pub fn increment_state_id(&mut self) -> i32 {
        self.state_id = (self.state_id + 1) & 32767;
        self.state_id
    }
    pub fn is_valid_slot_index(&self, index: i32) -> bool {
        // Vanilla accepts all negative values here; doClick decides which input
        // modes ignore them and which fail when they actually index a slot.
        index < self.slots.len() as i32
    }

    pub(crate) fn container<'a>(
        &'a self,
        ctx: &'a MenuContext<'_>,
        id: ContainerId,
    ) -> &'a dyn Container {
        match id {
            ContainerId::Player => &ctx.player,
            ContainerId::Local(i) => &self.local[i],
            ContainerId::External(i) => &*ctx.external[i],
        }
    }
    pub(crate) fn container_mut<'a>(
        &'a mut self,
        ctx: &'a mut MenuContext<'_>,
        id: ContainerId,
    ) -> &'a mut dyn Container {
        match id {
            ContainerId::Player => &mut ctx.player,
            ContainerId::Local(i) => &mut self.local[i],
            ContainerId::External(i) => &mut *ctx.external[i],
        }
    }
    pub fn get_item(&self, ctx: &MenuContext<'_>, index: usize) -> ItemStack {
        let slot = &self.slots[index];
        self.container(ctx, slot.container)
            .get_item(slot.container_slot)
    }
    pub fn set_item(&mut self, ctx: &mut MenuContext<'_>, index: usize, stack: ItemStack) {
        let slot = self.slots[index].clone();
        slot.set_by_player(self.container_mut(ctx, slot.container), stack);
    }
    pub fn items(&self, ctx: &MenuContext<'_>) -> Vec<ItemStack> {
        (0..self.slots.len())
            .map(|i| self.get_item(ctx, i))
            .collect()
    }
    pub fn still_valid(&self, ctx: &MenuContext<'_>) -> bool {
        self.slots
            .iter()
            .all(|slot| self.container(ctx, slot.container).still_valid())
    }
    /// Port of ServerPlayer's ContainerSynchronizer installation.
    pub fn set_synchronizer(&mut self, ctx: &MenuContext<'_>) {
        super::operation::set_menu_synchronizer(&mut super::operation::PureMenuSynchronization {
            menu: self,
            context: ctx,
            copies: None,
        })
        .expect("in-memory menu synchronization");
    }
    pub fn send_all_data_to_remote(&mut self, ctx: &MenuContext<'_>) {
        super::operation::send_all_menu_data(&mut super::operation::PureMenuSynchronization {
            menu: self,
            context: ctx,
            copies: None,
        })
        .expect("in-memory menu synchronization");
    }
    pub fn broadcast_changes(&mut self, ctx: &MenuContext<'_>) -> Result<()> {
        super::operation::broadcast_menu_changes(&mut super::operation::PureMenuSynchronization {
            menu: self,
            context: ctx,
            copies: None,
        })
    }
    pub fn broadcast_full_state(&mut self, ctx: &MenuContext<'_>) {
        super::operation::broadcast_menu_full_state(
            &mut super::operation::PureMenuSynchronization {
                menu: self,
                context: ctx,
                copies: None,
            },
        )
        .expect("in-memory menu synchronization");
    }
    pub fn suppress_remote_updates(&mut self) {
        self.suppressed = true;
    }
    pub fn resume_remote_updates(&mut self) {
        self.suppressed = false;
    }
    pub fn set_remote_slot(&mut self, index: usize, stack: &ItemStack) {
        self.remote_slots[index].force(stack);
    }
    pub fn set_remote_slot_unsafe(&mut self, index: i32, hash: HashedStack) {
        if let Some(slot) = self.remote_slots.get_mut(index as usize) {
            slot.receive(hash);
        }
    }
    pub fn set_remote_carried(&mut self, hash: HashedStack) {
        self.remote_carried.receive(hash);
    }
    pub fn drain_packets(&mut self) -> Vec<ferraloom_protocol::play::clientbound::Packet> {
        std::mem::take(&mut self.updates)
            .into_iter()
            .flat_map(ContainerUpdate::into_packets)
            .collect()
    }
    pub fn can_item_quick_replace(
        target: &ItemStack,
        stack: &ItemStack,
        ignore_size: bool,
    ) -> bool {
        target.is_empty()
            || is_same_item_same_components(target, stack)
                && count(target) + if ignore_size { 0 } else { count(stack) }
                    <= max_stack_size(stack)
    }
    pub fn quickcraft_mask(header: i32, kind: i32) -> i32 {
        (header & 3) | ((kind & 3) << 2)
    }
    pub(super) fn reset_quick_craft(&mut self) {
        self.quickcraft_status = 0;
        self.clear_quick_craft_slots();
    }
    pub(super) fn clear_quick_craft_slots(&mut self) {
        self.quickcraft_slots.clear();
        self.quickcraft_mod_count = self.quickcraft_mod_count.wrapping_add(1);
    }
    pub(super) fn can_take_for_pick_all(&self, slot: &Slot) -> bool {
        match self.kind {
            MenuKind::Merchant => false,
            MenuKind::Inventory
            | MenuKind::Crafting
            | MenuKind::Stonecutter
            | MenuKind::Cartography
            | MenuKind::Smithing => !matches!(slot.kind, SlotKind::Result(_)),
            _ => true,
        }
    }
    pub(crate) fn on_take(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: usize,
        stack: &ItemStack,
        quick_count: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.slots[index].clone();
        if slot.kind == SlotKind::Result(ResultKind::Crafting) && hooks.has_crafting() {
            self.take_crafting_result(ctx, stack, quick_count, hooks)
        } else {
            slot.on_take(
                self.container_mut(ctx, slot.container),
                stack,
                quick_count,
                hooks,
            )
        }
    }
    /// Port of AbstractContainerMenu#moveItemStackTo. The occupied-stack pass
    /// deliberately does not call mayPlace, matching vanilla's transfer behavior.
    pub fn move_item_stack_to(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: &mut ItemStack,
        start: usize,
        end: usize,
        backwards: bool,
        hooks: &mut dyn MenuHooks,
    ) -> Result<bool> {
        super::transfer::move_menu_item_stack_to(
            &mut super::transfer::PureMenuTransfer {
                menu: self,
                context: ctx,
                source: stack,
            },
            super::transfer::PureStackRef::Source,
            start,
            end,
            backwards,
            hooks,
        )
    }
    /// Port of AbstractContainerMenu#clicked/doClick for all seven ContainerInput values.
    pub fn clicked(
        &mut self,
        ctx: &mut MenuContext<'_>,
        slot_index: i32,
        button: i32,
        input: ContainerInput,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        // Result calculation is a synchronous recipe/enchantment/trade operation.
        // Until its owning workstream supplies that operation, reject affected
        // clicks before touching the canonical stacks.
        let inputs = match self.kind {
            MenuKind::Inventory => 5,
            MenuKind::Crafting => 10,
            MenuKind::Anvil | MenuKind::Grindstone | MenuKind::Cartography | MenuKind::Merchant => {
                3
            }
            MenuKind::Loom | MenuKind::Smithing => 4,
            MenuKind::Enchantment | MenuKind::Stonecutter => 2,
            _ => 0,
        };
        if inputs > 0
            && input != ContainerInput::Clone
            && (slot_index >= 0 && (slot_index as usize) < inputs
                || self.kind != MenuKind::Inventory && input == ContainerInput::QuickMove
                || input == ContainerInput::PickupAll
                    && (0..inputs).any(|i| !self.get_item(ctx, i).is_empty()))
        {
            hooks.prepare_recipe_click(self.kind)?;
        }
        click_menu(
            &mut super::in_memory::InMemoryMenuOperation::new(self, ctx),
            slot_index,
            button,
            input,
            hooks,
        )
    }
    pub(super) fn can_drag_to(&self, _slot: &Slot) -> bool {
        true
    }
}
