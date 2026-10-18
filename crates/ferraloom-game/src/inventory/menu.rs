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
    pub kind: MenuKind,
    pub container_id: i32,
    pub slots: Vec<Slot>,
    pub local: Vec<MenuContainer>,
    pub data: Vec<i32>,
    pub carried: ItemStack,
    state_id: i32,
    quickcraft_type: i32,
    quickcraft_status: i32,
    quickcraft_slots: Vec<usize>,
    last_slots: Vec<ItemStack>,
    last_data: Vec<i32>,
    remote_slots: Vec<RemoteSlot>,
    remote_carried: RemoteSlot,
    remote_data: Vec<i32>,
    synchronized: bool,
    suppressed: bool,
    pub changes: Vec<ContainerChange>,
    pub updates: Vec<ContainerUpdate>,
}
impl AbstractContainerMenu {
    pub(crate) fn empty(kind: MenuKind, container_id: i32) -> Self {
        Self {
            kind,
            container_id,
            slots: Vec::new(),
            local: Vec::new(),
            data: Vec::new(),
            carried: ItemStack::default(),
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
        }
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
        self.carried = carried;
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
    fn container<'a>(&'a self, ctx: &'a MenuContext<'_>, id: ContainerId) -> &'a dyn Container {
        match id {
            ContainerId::Player => &ctx.player,
            ContainerId::Local(i) => &self.local[i],
            ContainerId::External(i) => &*ctx.external[i],
        }
    }
    fn container_mut<'a>(
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
        self.synchronized = true;
        self.remote_carried = RemoteSlot::Unknown;
        self.remote_slots.fill(RemoteSlot::Unknown);
        self.send_all_data_to_remote(ctx);
    }
    pub fn send_all_data_to_remote(&mut self, ctx: &MenuContext<'_>) {
        let items = self.items(ctx);
        for (slot, stack) in self.remote_slots.iter_mut().zip(&items) {
            slot.force(stack);
        }
        self.remote_carried.force(&self.carried);
        self.remote_data.clone_from(&self.data);
        if self.synchronized {
            let state_id = self.increment_state_id();
            self.updates.push(ContainerUpdate::Initial {
                container_id: self.container_id,
                state_id,
                items,
                carried: self.carried.clone(),
                data: self.data.clone(),
            });
        }
    }
    pub fn broadcast_changes(&mut self, ctx: &MenuContext<'_>) -> Result<()> {
        self.trigger_listeners(ctx);
        if self.suppressed {
            return Ok(());
        }
        for i in 0..self.slots.len() {
            let current = self.get_item(ctx, i);
            if !self.remote_slots[i].matches(&current)? {
                self.remote_slots[i].force(&current);
                if self.synchronized {
                    let state_id = self.increment_state_id();
                    self.updates.push(ContainerUpdate::Slot {
                        container_id: self.container_id,
                        state_id,
                        slot: i,
                        stack: current,
                    });
                }
            }
        }
        if !self.remote_carried.matches(&self.carried)? {
            self.remote_carried.force(&self.carried);
            if self.synchronized {
                self.updates
                    .push(ContainerUpdate::Carried(self.carried.clone()));
            }
        }
        for i in 0..self.data.len() {
            if self.remote_data[i] != self.data[i] {
                self.remote_data[i] = self.data[i];
                if self.synchronized {
                    self.updates.push(ContainerUpdate::Data {
                        container_id: self.container_id,
                        id: i,
                        value: self.data[i],
                    });
                }
            }
        }
        Ok(())
    }
    fn trigger_listeners(&mut self, ctx: &MenuContext<'_>) {
        for i in 0..self.slots.len() {
            let current = self.get_item(ctx, i);
            if !matches(&self.last_slots[i], &current) {
                self.last_slots[i] = current.clone();
                self.changes.push(ContainerChange::Slot(i, current));
            }
        }
        for i in 0..self.data.len() {
            if self.last_data[i] != self.data[i] {
                self.last_data[i] = self.data[i];
                self.changes.push(ContainerChange::Data(i, self.data[i]));
            }
        }
    }
    pub fn broadcast_full_state(&mut self, ctx: &MenuContext<'_>) {
        self.trigger_listeners(ctx);
        self.send_all_data_to_remote(ctx);
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
    fn safe_take(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: usize,
        amount: i32,
        max_amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<ItemStack> {
        let slot = self.slots[index].clone();
        let creative = ctx.creative;
        slot.safe_take(
            self.container_mut(ctx, slot.container),
            amount,
            max_amount,
            creative,
            hooks,
        )
    }
    fn on_take(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: usize,
        stack: &ItemStack,
        quick_count: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.slots[index].clone();
        slot.on_take(
            self.container_mut(ctx, slot.container),
            stack,
            quick_count,
            hooks,
        )
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
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        let indices: Vec<_> = if backwards {
            (start..end).rev().collect()
        } else {
            (start..end).collect()
        };
        let mut changed = false;
        if is_stackable(stack) {
            for &i in &indices {
                if stack.is_empty() {
                    break;
                }
                let mut target = self.get_item(ctx, i);
                if !target.is_empty() && is_same_item_same_components(stack, &target) {
                    let slot = &self.slots[i];
                    let max = slot.max_stack_size(self.container(ctx, slot.container), &target);
                    let total = count(&target) + count(stack);
                    if total <= max {
                        set_count(stack, 0);
                        set_count(&mut target, total);
                        self.set_item(ctx, i, target);
                        changed = true;
                    } else if count(&target) < max {
                        let left = count(stack) - (max - count(&target));
                        set_count(stack, left);
                        set_count(&mut target, max);
                        self.set_item(ctx, i, target);
                        changed = true;
                    }
                }
            }
        }
        if !stack.is_empty() {
            for i in indices {
                let slot = &self.slots[i];
                if self.get_item(ctx, i).is_empty() && slot.may_place(stack, hooks)? {
                    let max = slot.max_stack_size(self.container(ctx, slot.container), stack);
                    let n = count(stack).min(max);
                    self.set_item(ctx, i, split(stack, n));
                    changed = true;
                    break;
                }
            }
        }
        Ok(changed)
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
                            std::mem::take(&mut self.carried)
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
                    self.carried = slot.safe_clone(self.container(ctx, slot.container));
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
                self.carried = slot.safe_insert(
                    self.container_mut(ctx, slot.container),
                    carried,
                    amount,
                    hooks,
                )?;
            }
        } else if slot.may_pickup(&clicked, ctx.creative, hooks)? {
            if self.carried.is_empty() {
                let amount = if button == 0 {
                    count(&clicked)
                } else {
                    (count(&clicked) + 1) / 2
                };
                let creative = ctx.creative;
                if let Some(taken) = slot.try_remove(
                    self.container_mut(ctx, slot.container),
                    amount,
                    i32::MAX,
                    creative,
                    hooks,
                )? {
                    self.carried = taken.clone();
                    self.on_take(ctx, slot.index, &taken, 0, hooks)?;
                }
            } else if slot.may_place(&self.carried, hooks)? {
                if is_same_item_same_components(&clicked, &self.carried) {
                    let amount = if button == 0 { count(&self.carried) } else { 1 };
                    let carried = self.carried.clone();
                    self.carried = slot.safe_insert(
                        self.container_mut(ctx, slot.container),
                        carried,
                        amount,
                        hooks,
                    )?;
                } else if count(&self.carried)
                    <= slot.max_stack_size(self.container(ctx, slot.container), &self.carried)
                {
                    let carried = std::mem::replace(&mut self.carried, clicked);
                    self.set_item(ctx, slot.index, carried);
                }
            } else if is_same_item_same_components(&clicked, &self.carried) {
                let max = max_stack_size(&self.carried) - count(&self.carried);
                let creative = ctx.creative;
                if let Some(taken) = slot.try_remove(
                    self.container_mut(ctx, slot.container),
                    count(&clicked),
                    max,
                    creative,
                    hooks,
                )? {
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
        if source.is_empty() {
            if !target.is_empty() && slot.may_pickup(&target, ctx.creative, hooks)? {
                ctx.player.set_item(button, target.clone());
                self.set_item(ctx, slot.index, ItemStack::default());
                self.on_take(ctx, slot.index, &target, count(&target), hooks)?;
            }
        } else if target.is_empty() {
            if slot.may_place(&source, hooks)? {
                let max = slot.max_stack_size(self.container(ctx, slot.container), &source);
                if count(&source) > max {
                    self.set_item(ctx, slot.index, split(&mut source, max));
                    ctx.player.set_item(button, source);
                } else {
                    ctx.player.set_item(button, ItemStack::default());
                    self.set_item(ctx, slot.index, source);
                }
            }
        } else if slot.may_pickup(&target, ctx.creative, hooks)?
            && slot.may_place(&source, hooks)?
        {
            let max = slot.max_stack_size(self.container(ctx, slot.container), &source);
            if count(&source) > max {
                self.set_item(ctx, slot.index, split(&mut source, max));
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
                self.set_item(ctx, slot.index, source);
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
                        self.set_item(ctx, *index, with_count(&source, new));
                    }
                }
                self.carried = with_count(&source, remaining);
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
