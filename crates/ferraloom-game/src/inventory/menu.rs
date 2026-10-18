use super::*;
use ferraloom_protocol::{component::HashedStack, play::ContainerInput};

/// Port of the stack and thrown-from-hand argument at a LivingEntity#drop call.
#[derive(Clone, Debug)]
pub struct DroppedStack {
    pub stack: ItemStack,
    pub thrown_from_hand: bool,
}

/// Live borrowed storage for one operation, supplied by the region owner.
pub struct MenuContext<'a> {
    pub player: PlayerContainer<'a>,
    pub external: &'a mut [&'a mut dyn Container],
    pub creative: bool,
    pub can_drop: bool,
    pub drops: Vec<DroppedStack>,
}
impl<'a> MenuContext<'a> {
    pub fn player(
        inventory: &'a mut crate::entity::player::inventory::Inventory,
        equipment: &'a mut crate::entity::player::equipment::EntityEquipment,
        creative: bool,
    ) -> Self {
        Self {
            player: PlayerContainer {
                inventory,
                equipment,
            },
            external: &mut [],
            creative,
            can_drop: true,
            drops: Vec::new(),
        }
    }
    fn drop_item(&mut self, stack: ItemStack) {
        if !stack.is_empty() {
            self.drops.push(DroppedStack {
                stack,
                thrown_from_hand: true,
            });
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
    pub data: Vec<i32>,
    pub carried: ItemStack,
    pub(crate) carried_generations: StackGenerations,
    state_id: i32,
    quickcraft_type: i32,
    quickcraft_status: i32,
    quickcraft_slots: Vec<usize>,
    pub(super) last_slots: Vec<ItemStack>,
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
            data: Vec::new(),
            carried: ItemStack::default(),
            carried_generations: StackGenerations::new(1),
            state_id: 0,
            quickcraft_type: -1,
            quickcraft_status: 0,
            quickcraft_slots: Vec::new(),
            last_slots: Vec::new(),
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
        self.last_slots.push(ItemStack::default());
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
    fn slot(&self, index: i32) -> Result<Slot> {
        self.slots
            .get(index as usize)
            .cloned()
            .ok_or(MenuError::InvalidSlot(index))
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
        })
        .expect("in-memory menu synchronization");
    }
    pub fn send_all_data_to_remote(&mut self, ctx: &MenuContext<'_>) {
        super::operation::send_all_menu_data(&mut super::operation::PureMenuSynchronization {
            menu: self,
            context: ctx,
        })
        .expect("in-memory menu synchronization");
    }
    pub fn broadcast_changes(&mut self, ctx: &MenuContext<'_>) -> Result<()> {
        super::operation::broadcast_menu_changes(&mut super::operation::PureMenuSynchronization {
            menu: self,
            context: ctx,
        })
    }
    pub fn broadcast_full_state(&mut self, ctx: &MenuContext<'_>) {
        super::operation::broadcast_menu_full_state(
            &mut super::operation::PureMenuSynchronization {
                menu: self,
                context: ctx,
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
    pub fn transfer_state(&mut self, other: &Self) {
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(j) = other.slots.iter().position(|s| {
                s.container == slot.container && s.container_slot == slot.container_slot
            }) {
                self.last_slots[i] = other.last_slots[j].clone();
                if self.synchronized && other.synchronized {
                    self.remote_slots[i] = other.remote_slots[j].clone();
                }
            }
        }
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
    fn reset_quick_craft(&mut self) {
        self.quickcraft_status = 0;
        self.quickcraft_slots.clear();
    }
    fn can_take_for_pick_all(&self, slot: &Slot) -> bool {
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
    fn try_remove_live(
        &mut self,
        ctx: &mut MenuContext<'_>,
        slot: &Slot,
        amount: i32,
        max_amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<Option<ItemStack>> {
        let stack = self.get_item(ctx, slot.index);
        if !slot.may_pickup(&stack, ctx.creative, hooks)?
            || (!slot.allow_modification(
                self.container(ctx, slot.container),
                ctx.creative,
                hooks,
            )? && max_amount < count(&stack))
        {
            return Ok(None);
        }
        if !stack.is_empty()
            && slot.kind == SlotKind::Result(ResultKind::Crafting)
            && let Some(crafting) = hooks.crafting_ref()
        {
            crafting.validate_take_result()?;
        }
        let result = self.remove_item_live(ctx, slot.index, amount.min(max_amount), hooks)?;
        if result.is_empty() {
            return Ok(None);
        }
        if self.get_item(ctx, slot.index).is_empty() {
            self.set_item_live(ctx, slot.index, ItemStack::default(), hooks)?;
        }
        Ok(Some(result))
    }
    fn safe_insert_live(
        &mut self,
        ctx: &mut MenuContext<'_>,
        slot: &Slot,
        mut input: ItemStack,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<ItemStack> {
        if input.is_empty() || !slot.may_place(&input, hooks)? {
            return Ok(input);
        }
        let mut target = self.get_item(ctx, slot.index);
        let n = amount
            .min(count(&input))
            .min(slot.max_stack_size(self.container(ctx, slot.container), &input) - count(&target));
        if n > 0 {
            if target.is_empty() {
                self.set_item_live(ctx, slot.index, split(&mut input, n), hooks)?;
            } else if is_same_item_same_components(&target, &input) {
                let left = count(&input) - n;
                let total = count(&target) + n;
                set_count(&mut input, left);
                set_count(&mut target, total);
                self.set_item_live(ctx, slot.index, target, hooks)?;
            }
        }
        Ok(input)
    }
    fn safe_take(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: usize,
        amount: i32,
        max_amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<ItemStack> {
        let slot = self.slots[index].clone();
        let result = self
            .try_remove_live(ctx, &slot, amount, max_amount, hooks)?
            .unwrap_or_default();
        if !result.is_empty() {
            self.on_take(ctx, index, &result, 0, hooks)?;
        }
        Ok(result)
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
        if input == ContainerInput::QuickCraft {
            return self.quick_craft(ctx, slot_index, button, hooks);
        }
        if self.quickcraft_status != 0 {
            self.reset_quick_craft();
            return Ok(());
        }
        match input {
            ContainerInput::Pickup | ContainerInput::QuickMove if button == 0 || button == 1 => {
                if slot_index == -999 {
                    if !self.carried.is_empty() {
                        let dropped = if button == 0 {
                            self.take_carried_unrooted()
                        } else {
                            split(&mut self.carried, 1)
                        };
                        ctx.drop_item(dropped);
                    }
                } else if slot_index >= 0 {
                    let slot = self.slot(slot_index)?;
                    let clicked = self.get_item(ctx, slot.index);
                    if input == ContainerInput::QuickMove {
                        if slot.may_pickup(&clicked, ctx.creative, hooks)? {
                            let mut moved = self.quick_move_stack(ctx, slot.index, hooks)?;
                            while !moved.is_empty()
                                && same_item(&self.get_item(ctx, slot.index), &moved)
                            {
                                moved = self.quick_move_stack(ctx, slot.index, hooks)?;
                            }
                        }
                    } else {
                        self.pickup(ctx, slot, button, hooks)?;
                    }
                }
            }
            ContainerInput::Swap if (0..9).contains(&button) || button == 40 => {
                self.swap(ctx, slot_index, button as usize, hooks)?
            }
            ContainerInput::Clone if ctx.creative && self.carried.is_empty() && slot_index >= 0 => {
                let slot = self.slot(slot_index)?;
                if !self.get_item(ctx, slot.index).is_empty() {
                    let cloned = slot.safe_clone(self.container(ctx, slot.container));
                    if slot.kind == SlotKind::Result(ResultKind::Crafting)
                        && let Some(crafting) = hooks.crafting()
                    {
                        crafting.item_crafted(self, ctx, &cloned, None)?;
                    }
                    self.set_carried(cloned);
                }
            }
            ContainerInput::Throw if self.carried.is_empty() && slot_index >= 0 => {
                let slot = self.slot(slot_index)?;
                let amount = if button == 0 {
                    1
                } else {
                    count(&self.get_item(ctx, slot.index))
                };
                if !ctx.can_drop {
                    return Ok(());
                }
                let mut dropped = self.safe_take(ctx, slot.index, amount, i32::MAX, hooks)?;
                ctx.drop_item(dropped.clone());
                if button == 1 {
                    while !dropped.is_empty()
                        && same_item(&self.get_item(ctx, slot.index), &dropped)
                    {
                        if !ctx.can_drop {
                            return Ok(());
                        }
                        dropped = self.safe_take(ctx, slot.index, amount, i32::MAX, hooks)?;
                        ctx.drop_item(dropped.clone());
                    }
                }
            }
            ContainerInput::PickupAll if slot_index >= 0 => {
                self.pickup_all(ctx, slot_index, button, hooks)?
            }
            _ => {}
        }
        Ok(())
    }
    fn pickup(
        &mut self,
        ctx: &mut MenuContext<'_>,
        slot: Slot,
        button: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let clicked = self.get_item(ctx, slot.index);
        // Bundle item overrides require their own contents/selection implementation.
        if [&clicked, &self.carried].iter().any(|stack| {
            crate::entity::player::inventory::component(
                stack,
                ferraloom_protocol::component::ComponentTypeId::BUNDLE_CONTENTS,
            )
            .is_some()
        }) {
            return Err(MenuError::Unsupported(
                "BundleItem inventory click override",
            ));
        }
        if clicked.is_empty() {
            if !self.carried.is_empty() {
                let amount = if button == 0 { count(&self.carried) } else { 1 };
                let carried = self.carried.clone();
                let remaining = self.safe_insert_live(ctx, &slot, carried, amount, hooks)?;
                self.set_carried(remaining);
            }
        } else if slot.may_pickup(&clicked, ctx.creative, hooks)? {
            if self.carried.is_empty() {
                let amount = if button == 0 {
                    count(&clicked)
                } else {
                    (count(&clicked) + 1) / 2
                };
                if let Some(taken) = self.try_remove_live(ctx, &slot, amount, i32::MAX, hooks)? {
                    self.set_carried(taken.clone());
                    self.on_take(ctx, slot.index, &taken, 0, hooks)?;
                }
            } else if slot.may_place(&self.carried, hooks)? {
                if is_same_item_same_components(&clicked, &self.carried) {
                    let amount = if button == 0 { count(&self.carried) } else { 1 };
                    let carried = self.carried.clone();
                    let remaining = self.safe_insert_live(ctx, &slot, carried, amount, hooks)?;
                    self.set_carried(remaining);
                } else if count(&self.carried)
                    <= slot.max_stack_size(self.container(ctx, slot.container), &self.carried)
                {
                    let carried = self
                        .carried_generations
                        .replace(0, &mut self.carried, clicked)
                        .into_owned()
                        .expect("live cursor replacement uses StackRef");
                    self.set_item_live(ctx, slot.index, carried, hooks)?;
                }
            } else if is_same_item_same_components(&clicked, &self.carried) {
                let max = max_stack_size(&self.carried) - count(&self.carried);
                if let Some(taken) =
                    self.try_remove_live(ctx, &slot, count(&clicked), max, hooks)?
                {
                    let total = count(&self.carried) + count(&taken);
                    set_count(&mut self.carried, total);
                    self.on_take(ctx, slot.index, &taken, 0, hooks)?;
                }
            }
        }
        self.container_mut(ctx, slot.container).set_changed();
        Ok(())
    }
    fn swap(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: i32,
        button: usize,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.slot(index)?;
        let mut source = ctx.player.get_item(button);
        let target = self.get_item(ctx, slot.index);
        if slot.container == ContainerId::Player
            && slot.container_slot == button
            && !source.is_empty()
        {
            // Java's source and target references alias here. Splitting an
            // oversized stack leaves the old target equal to the overflow,
            // which Inventory.add reinserts after the slot receives its limit.
            if slot.may_pickup(&source, ctx.creative, hooks)? && slot.may_place(&source, hooks)? {
                let max = slot.max_stack_size(self.container(ctx, slot.container), &source);
                if count(&source) > max {
                    self.set_item_live(ctx, slot.index, split(&mut source, max), hooks)?;
                    self.on_take(ctx, slot.index, &source, 0, hooks)?;
                    if !ctx
                        .player
                        .inventory
                        .add(&mut source, ctx.player.equipment, ctx.creative)
                    {
                        ctx.drop_item(source);
                    }
                } else {
                    self.set_item_live(ctx, slot.index, source.clone(), hooks)?;
                    self.on_take(ctx, slot.index, &source, 0, hooks)?;
                }
            }
            return Ok(());
        }
        if source.is_empty() {
            if !target.is_empty() && slot.may_pickup(&target, ctx.creative, hooks)? {
                if slot.kind == SlotKind::Result(ResultKind::Crafting)
                    && let Some(crafting) = hooks.crafting_ref()
                {
                    crafting.validate_take_result()?;
                }
                ctx.player.set_item(button, target.clone());
                self.set_item_live(ctx, slot.index, ItemStack::default(), hooks)?;
                self.on_take(ctx, slot.index, &target, count(&target), hooks)?;
            }
        } else if target.is_empty() {
            if slot.may_place(&source, hooks)? {
                let max = slot.max_stack_size(self.container(ctx, slot.container), &source);
                if count(&source) > max {
                    self.set_item_live(ctx, slot.index, split(&mut source, max), hooks)?;
                    ctx.player.set_item(button, source);
                } else {
                    ctx.player.set_item(button, ItemStack::default());
                    self.set_item_live(ctx, slot.index, source, hooks)?;
                }
            }
        } else if slot.may_pickup(&target, ctx.creative, hooks)?
            && slot.may_place(&source, hooks)?
        {
            let max = slot.max_stack_size(self.container(ctx, slot.container), &source);
            if count(&source) > max {
                self.set_item_live(ctx, slot.index, split(&mut source, max), hooks)?;
                ctx.player.set_item(button, source);
                self.on_take(ctx, slot.index, &target, 0, hooks)?;
                let mut target = target;
                if !ctx
                    .player
                    .inventory
                    .add(&mut target, ctx.player.equipment, ctx.creative)
                {
                    ctx.drop_item(target);
                }
            } else {
                ctx.player.set_item(button, target.clone());
                self.set_item_live(ctx, slot.index, source, hooks)?;
                self.on_take(ctx, slot.index, &target, 0, hooks)?;
            }
        }
        Ok(())
    }
    fn quick_craft(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: i32,
        button: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let expected = self.quickcraft_status;
        self.quickcraft_status = button & 3;
        if (expected != 1 || self.quickcraft_status != 2) && expected != self.quickcraft_status
            || self.carried.is_empty()
        {
            self.reset_quick_craft();
            return Ok(());
        }
        match self.quickcraft_status {
            0 => {
                self.quickcraft_type = (button >> 2) & 3;
                if self.quickcraft_type == 0
                    || self.quickcraft_type == 1
                    || self.quickcraft_type == 2 && ctx.creative
                {
                    self.quickcraft_status = 1;
                    self.quickcraft_slots.clear();
                } else {
                    self.reset_quick_craft();
                }
            }
            1 => {
                let slot = self.slot(index)?;
                if Self::can_item_quick_replace(
                    &self.get_item(ctx, slot.index),
                    &self.carried,
                    true,
                ) && slot.may_place(&self.carried, hooks)?
                    && (self.quickcraft_type == 2
                        || count(&self.carried) > self.quickcraft_slots.len() as i32)
                    && self.can_drag_to(&slot)
                    && !self.quickcraft_slots.contains(&slot.index)
                {
                    self.quickcraft_slots.push(slot.index);
                }
            }
            2 => {
                if self.quickcraft_slots.len() == 1 {
                    let index = self.quickcraft_slots[0];
                    self.reset_quick_craft();
                    return self.clicked(
                        ctx,
                        index as i32,
                        self.quickcraft_type,
                        ContainerInput::Pickup,
                        hooks,
                    );
                }
                let source = self.carried.clone();
                let mut remaining = count(&source);
                let slots = self.quickcraft_slots.clone();
                for index in &slots {
                    let slot = self.slots[*index].clone();
                    if Self::can_item_quick_replace(
                        &self.get_item(ctx, *index),
                        &self.carried,
                        true,
                    ) && slot.may_place(&self.carried, hooks)?
                        && (self.quickcraft_type == 2 || count(&self.carried) >= slots.len() as i32)
                        && self.can_drag_to(&slot)
                    {
                        let old = count(&self.get_item(ctx, *index));
                        let max = max_stack_size(&source)
                            .min(slot.max_stack_size(self.container(ctx, slot.container), &source));
                        let added = match self.quickcraft_type {
                            0 => (count(&source) as f32 / slots.len() as f32).floor() as i32,
                            1 => 1,
                            2 => max_stack_size(&source),
                            _ => count(&source),
                        };
                        let new = (added + old).min(max);
                        remaining -= new - old;
                        self.set_item_live(ctx, *index, with_count(&source, new), hooks)?;
                    }
                }
                self.set_carried(with_count(&source, remaining));
                self.reset_quick_craft();
            }
            _ => self.reset_quick_craft(),
        }
        Ok(())
    }
    fn can_drag_to(&self, _slot: &Slot) -> bool {
        true
    }
    fn pickup_all(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: i32,
        button: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.slot(index)?;
        let clicked = self.get_item(ctx, slot.index);
        if self.carried.is_empty()
            || !clicked.is_empty() && slot.may_pickup(&clicked, ctx.creative, hooks)?
        {
            return Ok(());
        }
        let indices: Vec<_> = if button == 0 {
            (0..self.slots.len()).collect()
        } else {
            (0..self.slots.len()).rev().collect()
        };
        for pass in 0..2 {
            for &index in &indices {
                if count(&self.carried) >= max_stack_size(&self.carried) {
                    break;
                }
                let slot = &self.slots[index];
                let stack = self.get_item(ctx, index);
                if !stack.is_empty()
                    && Self::can_item_quick_replace(&stack, &self.carried, true)
                    && slot.may_pickup(&stack, ctx.creative, hooks)?
                    && self.can_take_for_pick_all(slot)
                    && (pass != 0 || count(&stack) != max_stack_size(&stack))
                {
                    let max = max_stack_size(&self.carried) - count(&self.carried);
                    let removed = self.safe_take(ctx, index, count(&stack), max, hooks)?;
                    let total = count(&self.carried) + count(&removed);
                    set_count(&mut self.carried, total);
                }
            }
        }
        Ok(())
    }
}
