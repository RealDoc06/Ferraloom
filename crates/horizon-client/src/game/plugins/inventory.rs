//! The inventory area: players' inventories, plugins' containers and the
//! menus that show them; container clicks, opens and closes.
use super::{DIMS, GameHost};
use crate::game::{Dim, Game, lan::LanHost};
use ferraloom_plugins::{Click, Drag, Holder, InventoryHost, ItemFacts, PlayerId, Side, Swap};
use horizon_client::{
    items::ItemStack,
    lan::{
        guest::Guest,
        menu::{Slots, VEHICLE_KEY_Y, input},
        plugin_containers,
    },
};
use horizon_protocol::packets::play::{self, Clientbound as C};
use horizon_world::nbt::Tag;

/// The y of the key a plugin container's menu is kept under: no block, so
/// no lid opens and no reach check closes it.
const PLUGIN_KEY_Y: i32 = -200_000;

/// The slots of a player as Bukkit's `PlayerInventory` numbers them.
struct Parts<'a> {
    inventory: &'a mut Vec<Option<ItemStack>>,
    armor: &'a mut [Option<ItemStack>; 4],
    offhand: &'a mut Option<ItemStack>,
    selected: &'a mut usize,
    carried: &'a mut Option<ItemStack>,
}

impl Parts<'_> {
    /// 0-35 the inventory (hotbar first), 36-39 boots to helmet, 40 the off hand.
    fn slot(&mut self, slot: usize) -> Option<&mut Option<ItemStack>> {
        match slot {
            0..=35 => self.inventory.get_mut(slot),
            36..=39 => Some(&mut self.armor[39 - slot]),
            40 => Some(&mut *self.offhand),
            _ => None,
        }
    }
}

impl GameHost<'_, '_> {
    /// The player's slots: the game's own while it is their turn, else their record's.
    fn parts(&mut self, id: PlayerId) -> Option<Parts<'_>> {
        if self.game.lan_acting_uuid == Some(id) {
            let g = &mut *self.game;
            return Some(Parts {
                inventory: &mut g.inventory,
                armor: &mut g.armor,
                offhand: &mut g.offhand,
                selected: &mut g.selected,
                carried: &mut g.carried,
            });
        }
        let g = self.placed(id)?;
        Some(Parts {
            inventory: &mut g.inventory,
            armor: &mut g.armor,
            offhand: &mut g.offhand,
            selected: &mut g.selected,
            carried: &mut g.carried,
        })
    }

    fn placed(&mut self, id: PlayerId) -> Option<&mut Guest> {
        let hub = self.hub()?;
        hub.hub.guests.iter_mut().find(|g| g.uuid == id && g.placed)
    }

    /// `f` with the game and the hub apart, wherever the hub is now.
    fn split<R>(&mut self, f: impl FnOnce(&mut Game, &mut LanHost) -> R) -> Option<R> {
        if let Some(hub) = self.hub.as_deref_mut() {
            return Some(f(self.game, hub));
        }
        let mut hub = self.game.lan.take()?;
        let r = f(self.game, &mut hub);
        self.game.lan = Some(hub);
        Some(r)
    }
}

impl InventoryHost for GameHost<'_, '_> {
    fn inventory_size(&mut self, at: Holder) -> Option<usize> {
        match at {
            Holder::Player(id) => self.parts(id).map(|_| 41),
            Holder::Ender(id) => self.placed(id).map(|g| g.ender_items.len()),
            Holder::Crafting(id) => self.placed(id).map(|_| 5),
            Holder::Custom(id) => plugin_containers::size(id),
            Holder::Menu(opening) => {
                let hub = self.hub()?;
                let g = &hub.hub.guests[opener(&hub.hub.guests, opening)?];
                g.menu.as_ref().map(|m| m.size)
            }
            Holder::Block { world, pos } => {
                let w = self.game.level_world(*DIMS.get(world)?)?;
                let e = w.block_entities.get(&(pos[0], pos[1], pos[2]))?;
                Some(e.slots().len())
            }
        }
    }

    fn slot(&mut self, at: Holder, slot: usize) -> Option<ItemStack> {
        match at {
            Holder::Player(id) => self.parts(id)?.slot(slot)?.clone(),
            Holder::Ender(id) => self.placed(id)?.ender_items.get(slot)?.clone(),
            Holder::Crafting(id) => self.placed(id).and_then(|g| g.menu_slot(slot)),
            Holder::Custom(id) => plugin_containers::get(id, slot),
            Holder::Menu(opening) => self
                .split(|game, hub| {
                    let g = &hub.hub.guests[opener(&hub.hub.guests, opening)?];
                    let m = g.menu.as_ref().filter(|m| slot < m.size)?;
                    // In the guest's own level, which may not be the one in place.
                    let world = game.level_world(Dim::from_id(&g.dimension))?;
                    m.slots.get(world, g, slot)
                })
                .flatten(),
            Holder::Block { world, pos } => {
                let w = self.game.level_world(*DIMS.get(world)?)?;
                let e = w.block_entities.get(&(pos[0], pos[1], pos[2]))?;
                e.slots().get(slot)?.clone()
            }
        }
    }

    fn set_slot(&mut self, at: Holder, slot: usize, item: Option<ItemStack>) -> bool {
        match at {
            Holder::Player(id) => match self.parts(id).as_mut().and_then(|p| p.slot(slot)) {
                Some(s) => {
                    *s = item;
                    true
                }
                None => false,
            },
            Holder::Ender(id) => match self.placed(id).and_then(|g| g.ender_items.get_mut(slot)) {
                Some(s) => {
                    *s = item;
                    true
                }
                None => false,
            },
            Holder::Crafting(id) => match self.placed(id) {
                Some(g) if slot < 5 => {
                    g.set_menu_slot(slot, item);
                    true
                }
                _ => false,
            },
            Holder::Custom(id) => plugin_containers::set(id, slot, item),
            Holder::Menu(opening) => self
                .split(|game, hub| {
                    let Some(i) = opener(&hub.hub.guests, opening) else {
                        return false;
                    };
                    let g = &mut hub.hub.guests[i];
                    let Some(m) = g.menu.as_ref().filter(|m| slot < m.size) else {
                        return false;
                    };
                    let (slots, size) = (m.slots, m.size);
                    // In the guest's own level, which may not be the one in place.
                    let dim = Dim::from_id(&g.dimension);
                    game.in_level(dim, |game| {
                        // A half replaced by a smaller container, or gone, since
                        // the opening: not the menu's slots any more.
                        let Some(mut items) =
                            slots.read(&game.world, g).filter(|i| i.len() == size)
                        else {
                            return false;
                        };
                        let Some(s) = items.get_mut(slot) else {
                            return false;
                        };
                        *s = item;
                        slots.write(&mut game.world, g, items);
                        if let Slots::Block(key) | Slots::Double(key, _) = slots {
                            game.container_changed(key);
                            // A vehicle's menu edits the vehicle's own items.
                            game.store_cart_items(key);
                        }
                        true
                    })
                    .unwrap_or(false)
                })
                .unwrap_or(false),
            Holder::Block { world, pos } => {
                let Some(&dim) = DIMS.get(world) else {
                    return false;
                };
                let key = (pos[0], pos[1], pos[2]);
                self.game
                    .in_level(dim, |game| {
                        let e = game.world.block_entities.get_mut(&key);
                        let Some(s) = e.and_then(|e| e.slots_mut().get_mut(slot)) else {
                            return false;
                        };
                        *s = item;
                        game.container_changed(key);
                        true
                    })
                    .unwrap_or(false)
            }
        }
    }

    fn held_slot(&mut self, player: PlayerId) -> Option<usize> {
        self.parts(player).map(|p| *p.selected)
    }

    fn set_held_slot(&mut self, player: PlayerId, slot: usize) -> bool {
        if slot > 8 {
            return false;
        }
        let Some(p) = self.parts(player) else {
            return false;
        };
        *p.selected = slot;
        // ServerPlayer's inventory.setSelectedSlot, told to the client at once.
        if let Some(g) = self.placed(player) {
            g.sent_selected = slot;
            g.send(&C::SetHeldSlot(play::SetHeldSlot { slot: slot as i32 }));
        }
        true
    }

    fn cursor(&mut self, player: PlayerId) -> Option<ItemStack> {
        self.parts(player)?.carried.clone()
    }

    fn set_cursor(&mut self, player: PlayerId, item: Option<ItemStack>) -> bool {
        let Some(p) = self.parts(player) else {
            return false;
        };
        *p.carried = item.clone();
        self.split(|_, hub| {
            let net = hub.hub.stack(&item);
            if let Some(g) = hub.hub.guests.iter_mut().find(|g| g.uuid == player) {
                g.send(&C::SetCursorItem(play::SetCursorItem { item: net }));
            }
        });
        true
    }

    fn create_container(&mut self, size: usize) -> u32 {
        plugin_containers::create(size)
    }

    fn drop_container(&mut self, id: u32) {
        plugin_containers::remove(id);
    }

    fn open_container(
        &mut self,
        player: PlayerId,
        at: Holder,
        menu: &str,
        title: Tag,
        retitle: bool,
    ) -> bool {
        let dim = super::world_index(self.game.dim);
        self.split(|game, hub| {
            let Some(gi) = hub
                .hub
                .guests
                .iter()
                .position(|g| g.uuid == player && g.placed)
            else {
                return false;
            };
            let g = &hub.hub.guests[gi];
            let (conn, open) = (g.conn, g.menu.is_some());
            // The player's clicks resolve the block in their own level.
            let own = super::world_index(Dim::from_id(&g.dimension));
            let (slots, pos) = match at {
                Holder::Custom(id) => (Slots::Custom(id), (i32::MIN, PLUGIN_KEY_Y, id as i32)),
                Holder::Block { world, pos } if world == dim && world == own => {
                    let key = (pos[0], pos[1], pos[2]);
                    (Slots::Block(key), key)
                }
                _ => return false,
            };
            let Some(items) = slots.read(&game.world, g) else {
                return false;
            };
            // ServerPlayer.openMenu closes the menu that was open, as close_container;
            // a retitle (CraftInventoryView.setTitle) keeps it and its cursor.
            if !retitle {
                game.guest_container_closed(hub, conn);
                let closed = open
                    || game.lan_station_closed(hub, conn, None)
                    || game.lan_crafting_closed(hub, conn, None)
                    || game.lan_merchant_closed(hub, conn, None)
                    || game.lan_mount_closed(hub, conn, None);
                if closed {
                    game.guest_carried_back(hub, gi);
                }
            }
            let opening = horizon_client::lan::menu::next_opening();
            hub.hub
                .open_container(conn, menu, title, pos, slots, items, opening);
            game.plugin_name_menu(hub, conn);
            true
        })
        .unwrap_or(false)
    }

    fn close_container(&mut self, player: PlayerId) -> bool {
        self.split(|game, hub| {
            let Some(gi) = hub
                .hub
                .guests
                .iter()
                .position(|g| g.uuid == player && g.placed)
            else {
                return false;
            };
            let conn = hub.hub.guests[gi].conn;
            let open = hub.hub.guests[gi].menu.is_some();
            // A menu the game runs itself closes with nothing told: its client is.
            let shown = engine_window(hub, conn);
            if open {
                game.guest_container_closed(hub, conn);
                hub.hub.close_container(conn);
            }
            let closed = open
                || game.lan_station_closed(hub, conn, None)
                || game.lan_crafting_closed(hub, conn, None)
                || game.lan_merchant_closed(hub, conn, None)
                || game.lan_mount_closed(hub, conn, None);
            if closed {
                game.guest_carried_back(hub, gi);
            }
            if let Some(container) = shown.filter(|_| !open) {
                hub.hub
                    .send_to(conn, &C::ContainerClose(play::ContainerClose { container }));
            }
            closed
        })
        .unwrap_or(false)
    }

    fn item_facts(&mut self, item: &str) -> Option<ItemFacts> {
        let i = self.game.res.items().get(item)?;
        Some(ItemFacts {
            max_stack: i.max_stack,
            max_damage: i.max_damage,
            edible: i.food.is_some(),
            fuel: i.fuel.is_some(),
            record: i.jukebox_song.is_some(),
            compostable: i.compostable.is_some(),
            remainder: i.crafting_remainder.clone(),
        })
    }

    fn open_menu(&mut self, player: PlayerId) -> Option<(Holder, &'static str)> {
        let g = self.placed(player)?;
        let m = g.menu.as_ref()?;
        Some((holder(g, m.slots, m.opening), m.inventory_type))
    }

    fn resend_inventory(&mut self, player: PlayerId) {
        self.split(|_, hub| {
            let Some(gi) = hub
                .hub
                .guests
                .iter()
                .position(|g| g.uuid == player && g.placed)
            else {
                return;
            };
            hub.hub.send_inventory(gi);
            // An open menu's slots all go again at the end of the tick.
            if let Some(m) = hub.hub.guests[gi].menu.as_mut() {
                m.sent.clear();
            }
        });
    }
}

/// The window of the menu the game runs itself that the guest has open (a
/// crafting table, a station or lectern, trading, a mount's), if any.
fn engine_window(host: &LanHost, conn: u32) -> Option<i32> {
    let p = host.play.get(&conn)?;
    p.crafting
        .table
        .as_ref()
        .map(|t| t.0)
        .or(p.station.as_ref().map(|s| s.window))
        .or(p.lectern.as_ref().map(|l| l.window))
        .or(p.merchant.as_ref().map(|m| m.window))
        .or(p.mount_window.as_ref().map(|m| m.window))
}

/// The `InventoryType` of a menu's top.
fn top_type(game: &Game, slots: Slots) -> &'static str {
    let key = match slots {
        Slots::Ender => return "ENDER_CHEST",
        Slots::Custom(_) => return "CHEST",
        Slots::Double(..) => return "CHEST",
        // A chest minecart or chest boat, or a hopper minecart.
        Slots::Block(key) if key.1 == VEHICLE_KEY_Y => {
            let hopper = game
                .world
                .block_entities
                .get(&key)
                .is_some_and(|e| e.slots().len() == 5);
            return if hopper { "HOPPER" } else { "CHEST" };
        }
        Slots::Block(key) => key,
    };
    let name = &game
        .res
        .facts()
        .get(game.world.block(key.0, key.1, key.2))
        .name;
    match name.trim_start_matches("minecraft:") {
        "barrel" => "BARREL",
        "dispenser" => "DISPENSER",
        "dropper" => "DROPPER",
        "hopper" => "HOPPER",
        "furnace" => "FURNACE",
        "blast_furnace" => "BLAST_FURNACE",
        "smoker" => "SMOKER",
        "brewing_stand" => "BREWING",
        "crafter" => "CRAFTER",
        n if n.ends_with("shulker_box") => "SHULKER_BOX",
        _ => "CHEST",
    }
}

/// The top of a guest's open menu as Bukkit names it.
fn top(g: &Guest, window: i32) -> Option<(Holder, &'static str, usize)> {
    if window == 0 {
        return Some((Holder::Crafting(g.uuid), "CRAFTING", 5));
    }
    let m = g.menu.as_ref().filter(|m| m.window == window)?;
    Some((holder(g, m.slots, m.opening), m.inventory_type, m.size))
}

/// The guest whose open menu is `opening`: none once that opening ended.
fn opener(guests: &[Guest], opening: u32) -> Option<usize> {
    guests
        .iter()
        .position(|g| g.placed && g.menu.as_ref().is_some_and(|m| m.opening == opening))
}

/// The holder of guest `g`'s menu over `slots`, opened as `opening`: a single
/// block's container is the block's, as its InventoryOpenEvent named it, in
/// the guest's own dimension (a change of level reports its close from the
/// next one); a double chest or a vehicle's container (kept under a key, not
/// a block) is that opening's.
fn holder(g: &Guest, slots: Slots, opening: u32) -> Holder {
    match slots {
        Slots::Custom(c) => Holder::Custom(c),
        Slots::Ender => Holder::Ender(g.uuid),
        Slots::Block(key) if key.1 == VEHICLE_KEY_Y => Holder::Menu(opening),
        Slots::Block(key) => Holder::Block {
            world: super::world_index(Dim::from_id(&g.dimension)),
            pos: [key.0, key.1, key.2],
        },
        Slots::Double(..) => Holder::Menu(opening),
    }
}

/// A closed menu as the bridge names it: the player, its top and its `InventoryType`.
pub(in crate::game) type ClosedMenu = (PlayerId, Holder, &'static str);

impl Game<'_> {
    /// What raw slot `slot` of the guest's menu `window` (its container `size` slots) holds.
    fn plugin_slot_item(
        &self,
        g: &Guest,
        window: i32,
        size: usize,
        slot: usize,
    ) -> Option<ItemStack> {
        if window == 0 {
            return g.menu_slot(slot);
        }
        let m = g.menu.as_ref()?;
        if slot < size {
            return m.slots.get(&self.world, g, slot);
        }
        let n = slot - size;
        let index = if n < 27 { 9 + n } else { n - 27 };
        g.inventory.get(index).cloned().flatten()
    }

    fn plugin_max_stack(&self, item: &ItemStack) -> u32 {
        self.res.items().get(&item.item).map_or(64, |i| i.max_stack)
    }

    /// `Slot.mayPlace` as the click reads it: an armour slot takes its
    /// armour only, a result slot nothing.
    fn plugin_may_place(&self, window: i32, slot: usize, item: &ItemStack) -> bool {
        if window != 0 {
            return true;
        }
        match slot {
            0 => false,
            5..=8 => {
                let wanted = ["head", "chest", "legs", "feet"][slot - 5];
                self.res
                    .items()
                    .get(&item.item)
                    .and_then(|i| i.equip_slot.as_deref())
                    == Some(wanted)
            }
            _ => true,
        }
    }

    /// CraftBukkit's reading of a container click
    /// (`ServerGamePacketListenerImpl.handleContainerClick`): its `ClickType`
    /// and `InventoryAction`, from the slots before the click.
    fn plugin_click_kind(
        &self,
        g: &Guest,
        p: &play::ContainerClick,
        size: usize,
    ) -> (&'static str, &'static str) {
        let window = p.container;
        let (slot, button) = (i32::from(p.slot), i32::from(p.button));
        let item = |slot: i32| self.plugin_slot_item(g, window, size, usize::try_from(slot).ok()?);
        let in_inventory = |slot: i32| {
            if window == 0 {
                slot >= 5
            } else {
                slot >= size as i32
            }
        };
        let slot_max = |slot: i32| {
            if window == 0 && (5..=8).contains(&slot) {
                1
            } else {
                99
            }
        };
        let carried = g.carried.as_ref();
        match p.input {
            input::PICKUP => {
                let click = match button {
                    0 => "LEFT",
                    1 => "RIGHT",
                    _ => return ("UNKNOWN", "UNKNOWN"),
                };
                let left = button == 0;
                let action = if slot == -999 {
                    match carried {
                        Some(_) if left => "DROP_ALL_CURSOR",
                        Some(_) => "DROP_ONE_CURSOR",
                        None => "NOTHING",
                    }
                } else if slot < 0 {
                    "NOTHING"
                } else {
                    match (item(slot), carried) {
                        (None, None) => "NOTHING",
                        (None, Some(_)) if left => "PLACE_ALL",
                        (None, Some(_)) => "PLACE_ONE",
                        (Some(_), None) if left => "PICKUP_ALL",
                        (Some(_), None) => "PICKUP_HALF",
                        (Some(clicked), Some(cursor))
                            if self.plugin_may_place(window, slot as usize, cursor) =>
                        {
                            if clicked.same_item_same_components(cursor) {
                                let wanted = if left { cursor.count as i64 } else { 1 };
                                let room =
                                    self.plugin_max_stack(&clicked) as i64 - clicked.count as i64;
                                let to_place = wanted.min(room).min(99 - clicked.count as i64);
                                match to_place {
                                    1 => "PLACE_ONE",
                                    n if n == cursor.count as i64 => "PLACE_ALL",
                                    -1 => "PICKUP_ONE",
                                    n if n < 0 => "PICKUP_SOME",
                                    0 => "NOTHING",
                                    _ => "PLACE_SOME",
                                }
                            } else if cursor.count <= slot_max(slot) {
                                "SWAP_WITH_CURSOR"
                            } else {
                                "NOTHING"
                            }
                        }
                        (Some(clicked), Some(cursor)) => {
                            if cursor.same_item_same_components(&clicked)
                                && clicked.count + cursor.count <= self.plugin_max_stack(cursor)
                            {
                                "PICKUP_ALL"
                            } else {
                                "NOTHING"
                            }
                        }
                    }
                };
                (click, action)
            }
            input::QUICK_MOVE => {
                let click = match button {
                    0 => "SHIFT_LEFT",
                    1 => "SHIFT_RIGHT",
                    _ => return ("UNKNOWN", "UNKNOWN"),
                };
                let action = if slot >= 0 && item(slot).is_some() {
                    "MOVE_TO_OTHER_INVENTORY"
                } else {
                    "NOTHING"
                };
                (click, action)
            }
            input::SWAP if (0..9).contains(&button) || button == 40 => {
                let click = if button == 40 {
                    "SWAP_OFFHAND"
                } else {
                    "NUMBER_KEY"
                };
                let hotbar = if button == 40 {
                    g.offhand.clone()
                } else {
                    g.inventory.get(button as usize).cloned().flatten()
                };
                let clicked = item(slot);
                let fits =
                    |h: &ItemStack| slot >= 0 && self.plugin_may_place(window, slot as usize, h);
                let action = match (&clicked, &hotbar) {
                    (Some(_), None) => "HOTBAR_SWAP",
                    (Some(_), Some(h)) if in_inventory(slot) && fits(h) => "HOTBAR_SWAP",
                    (Some(_), Some(_)) => "HOTBAR_MOVE_AND_READD",
                    (None, Some(h)) if fits(h) => "HOTBAR_SWAP",
                    _ => "NOTHING",
                };
                (click, action)
            }
            input::CLONE if button == 2 => {
                let clone = slot >= 0 && item(slot).is_some() && g.creative() && carried.is_none();
                ("MIDDLE", if clone { "CLONE_STACK" } else { "NOTHING" })
            }
            input::THROW if slot >= 0 => {
                let some = item(slot).is_some();
                match button {
                    0 => ("DROP", if some { "DROP_ONE_SLOT" } else { "NOTHING" }),
                    1 => (
                        "CONTROL_DROP",
                        if some { "DROP_ALL_SLOT" } else { "NOTHING" },
                    ),
                    _ => ("UNKNOWN", "UNKNOWN"),
                }
            }
            input::THROW => (if button == 1 { "RIGHT" } else { "LEFT" }, "NOTHING"),
            input::PICKUP_ALL => {
                let collect = slot >= 0
                    && carried.is_some_and(|c| {
                        let total = if window == 0 { 46 } else { size as i32 + 36 };
                        (0..total).any(|s| item(s).is_some_and(|i| i.item == c.item))
                    });
                (
                    "DOUBLE_CLICK",
                    if collect {
                        "COLLECT_TO_CURSOR"
                    } else {
                        "NOTHING"
                    },
                )
            }
            _ => ("UNKNOWN", "UNKNOWN"),
        }
    }

    /// CraftBukkit's InventoryClickEvent in handleContainerClick, before
    /// `AbstractContainerMenu.clicked`: true when a plugin cancelled the
    /// click, which then changes nothing and the client is sent the clicked
    /// window again (`resync_cancelled`). Drags
    /// (`QUICK_CRAFT`) fire InventoryDragEvent instead; menus the game runs
    /// itself (crafting table, anvil, trading...) fire nothing yet.
    pub(in crate::game) fn plugin_click(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        p: &play::ContainerClick,
    ) -> bool {
        if p.input == input::QUICK_CRAFT {
            return false;
        }
        let Some(g) = host.hub.guest(conn).filter(|g| g.placed && !g.spectator()) else {
            return false;
        };
        let Some((top, top_type, size)) = top(g, p.container) else {
            return false;
        };
        let (click, action) = self.plugin_click_kind(g, p, size);
        let event = Click {
            player: g.uuid,
            top,
            top_type,
            raw_slot: i32::from(p.slot),
            click,
            action,
            hotbar_key: if click == "NUMBER_KEY" {
                i32::from(p.button)
            } else {
                -1
            },
        };
        let cancelled =
            ferraloom_plugins::inventory_click(&mut GameHost::new(self, Some(host)), &event);
        if cancelled {
            self.resync_cancelled(host, conn, p);
        }
        cancelled
    }

    /// CraftBukkit's InventoryDragEvent in `AbstractContainerMenu.doClick`, as
    /// a drag's last packet arrives and before anything lands: true when a
    /// plugin denied it, and the drag ends with nothing moved and the client
    /// corrected. The slots get what the drag would give them (an even split,
    /// one each, or a full stack), as the event reports them.
    pub(in crate::game) fn plugin_drag(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        p: &play::ContainerClick,
    ) -> bool {
        if p.input != input::QUICK_CRAFT || p.button & 3 != 2 {
            return false;
        }
        let Some(g) = host.hub.guest(conn).filter(|g| g.placed && !g.spectator()) else {
            return false;
        };
        let Some((top, top_type, size)) = top(g, p.container) else {
            return false;
        };
        let drag = if p.container == 0 {
            g.drag.as_ref()
        } else {
            g.menu.as_ref().and_then(|m| m.drag.as_ref())
        };
        let (Some((kind, slots)), Some(carried)) = (drag, g.carried.as_ref()) else {
            return false;
        };
        let n = slots.len() as u32;
        if n == 0 || (*kind != 2 && carried.count < n) {
            return false;
        }
        let each = match kind {
            0 => carried.count / n,
            1 => 1,
            _ => self.plugin_max_stack(carried),
        };
        let mut left = carried.count;
        let mut placed = Vec::new();
        for &slot in slots {
            let existing = self
                .plugin_slot_item(g, p.container, size, slot)
                .map_or(0, |s| s.count);
            let armor = p.container == 0 && (5..=8).contains(&slot);
            let max = if armor {
                1
            } else {
                self.plugin_max_stack(carried)
            };
            let count = (each + existing).min(max);
            left = left.saturating_sub(count.saturating_sub(existing));
            placed.push((
                slot as i32,
                ItemStack {
                    count,
                    ..carried.clone()
                },
            ));
        }
        if *kind == 2 && g.creative() {
            left = carried.count;
        }
        let new_cursor = (left > 0).then(|| ItemStack {
            count: left,
            ..carried.clone()
        });
        let (old, id) = (carried.clone(), g.uuid);
        let event = Drag {
            player: id,
            top,
            top_type,
            slots: &placed,
            new_cursor: new_cursor.as_ref(),
            old_cursor: &old,
            right: *kind == 1,
        };
        let denied =
            ferraloom_plugins::inventory_drag(&mut GameHost::new(self, Some(host)), &event);
        if denied {
            if let Some(g) = host.hub.guests.iter_mut().find(|g| g.conn == conn) {
                g.drag = None;
                if let Some(m) = g.menu.as_mut() {
                    m.drag = None;
                }
            }
            self.resync_cancelled(host, conn, p);
        }
        denied
    }

    /// After a cancelled click or drag, CraftBukkit's resend: whatever the client
    /// predicted (moves, swaps, renamed stacks) is undone by sending whole the
    /// player's inventory when no menu is open or the click was in it, else the
    /// clicked window and the cursor if it is still the open one. A listener
    /// that opened another menu has sent it already. After an off-hand swap
    /// (key F) the inventory goes too, whatever menu is open now: no
    /// container menu shows the off hand.
    fn resync_cancelled(&self, host: &mut LanHost, conn: u32, p: &play::ContainerClick) {
        let Some(i) = host.hub.guest_index(conn) else {
            return;
        };
        let inventory_sent = match (
            p.container,
            host.hub.guests[i].menu.as_ref().map(|m| m.window),
        ) {
            (0, _) | (_, None) => {
                host.hub.send_inventory(i);
                true
            }
            (w, Some(open)) if w == open => {
                host.hub.resend_menu(i, &self.world);
                false
            }
            _ => false,
        };
        if !inventory_sent && p.input == input::SWAP && p.button == 40 {
            host.hub.send_inventory(i);
        }
    }

    /// InventoryOpenEvent as a guest's container is about to open
    /// (`ServerPlayer.openMenu`) as `opening`: true when a plugin cancelled it.
    /// A vehicle's container is named by that opening, so it reads as nothing
    /// until it is open.
    pub(in crate::game) fn plugin_opening(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        slots: Slots,
        opening: u32,
    ) -> bool {
        let Some(id) = host.hub.guest(conn).map(|g| g.uuid) else {
            return false;
        };
        let holder = match slots {
            Slots::Custom(c) => Holder::Custom(c),
            Slots::Ender => Holder::Ender(id),
            Slots::Block(key) if key.1 == VEHICLE_KEY_Y => Holder::Menu(opening),
            Slots::Block(key) | Slots::Double(key, _) => Holder::Block {
                world: super::world_index(self.dim),
                pos: [key.0, key.1, key.2],
            },
        };
        let kind = top_type(self, slots);
        ferraloom_plugins::inventory_open(&mut GameHost::new(self, Some(host)), id, holder, kind)
    }

    /// InventoryCloseEvent as the client closes its window (`handleContainerClose`),
    /// told whenever plugins run so the bridge forgets the menu even when nobody
    /// listens. Always false: the close goes on.
    pub(in crate::game) fn plugin_closed(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        window: i32,
    ) -> bool {
        let Some(g) = host.hub.guest(conn).filter(|g| g.placed) else {
            return false;
        };
        // A stale close of the inventory while a container is open closes nothing.
        if window == 0 && g.menu.is_some() {
            return false;
        }
        let Some((top, kind, _)) = top(g, window) else {
            return false;
        };
        let id = g.uuid;
        ferraloom_plugins::inventory_close(&mut GameHost::new(self, Some(host)), id, top, kind);
        false
    }

    /// The guest's open container as the bridge names it, while plugins run: a
    /// close the server makes by itself (out of reach, a respawn, another level)
    /// reads it before and tells it after (`plugin_menu_gone`).
    pub(in crate::game) fn plugin_open_menu(
        &self,
        host: &LanHost,
        conn: u32,
    ) -> Option<ClosedMenu> {
        if !ferraloom_plugins::running() {
            return None;
        }
        let g = host.hub.guest(conn).filter(|g| g.placed)?;
        let m = g.menu.as_ref()?;
        Some((g.uuid, holder(g, m.slots, m.opening), m.inventory_type))
    }

    /// Names the guest's container just opened as Bukkit does, while plugins
    /// run, for the events and the bridge's questions until it closes.
    pub(in crate::game) fn plugin_name_menu(&self, host: &mut LanHost, conn: u32) {
        if !ferraloom_plugins::running() {
            return;
        }
        if let Some(m) = host.hub.guest_mut(conn).and_then(|g| g.menu.as_mut()) {
            m.inventory_type = top_type(self, m.slots);
        }
    }

    /// The container over `slots` (its `opening`, of `InventoryType` `kind`) closed because
    /// its block went (`HostEvent::MenuGone`).
    pub(in crate::game) fn plugin_menu_block_gone(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        slots: Slots,
        opening: u32,
        kind: &'static str,
    ) {
        if let Some(g) = host.hub.guest(conn).filter(|g| g.placed) {
            let menu = (g.uuid, holder(g, slots, opening), kind);
            self.plugin_menu_gone(host, menu);
        }
    }

    /// `PlayerList.remove`'s `closeContainer`, before PlayerQuitEvent: the
    /// InventoryCloseEvent of the menu `guest` (gone from the hub already) had open.
    pub(in crate::game) fn plugin_left_menu(&mut self, host: &mut LanHost, guest: &Guest) {
        if !ferraloom_plugins::running() || !guest.placed {
            return;
        }
        if let Some(m) = guest.menu.as_ref() {
            let menu = (
                guest.uuid,
                holder(guest, m.slots, m.opening),
                m.inventory_type,
            );
            self.plugin_menu_gone(host, menu);
        }
    }

    /// CraftBukkit's InventoryCloseEvent for a menu the server closed by itself
    /// (`ServerPlayer.closeContainer`), after it closed.
    pub(in crate::game) fn plugin_menu_gone(&mut self, host: &mut LanHost, menu: ClosedMenu) {
        let (id, top, kind) = menu;
        ferraloom_plugins::inventory_close(&mut GameHost::new(self, Some(host)), id, top, kind);
    }

    /// HopperBlockEntity's InventoryMoveItemEvent for `item` (one item, taken
    /// out already) moving between the block containers at `from` and `to`:
    /// false when a plugin cancelled it, and the caller puts the item back.
    /// The plugin may change the item that moves.
    pub(in crate::game) fn plugin_move_item(
        &mut self,
        from: (i32, i32, i32),
        to: (i32, i32, i32),
        source_initiated: bool,
        item: &mut ItemStack,
    ) -> bool {
        let world = super::world_index(self.dim);
        let side = |game: &Game<'_>, p: (i32, i32, i32)| Side {
            pos: [p.0, p.1, p.2],
            kind: top_type(game, Slots::Block(p)),
        };
        let (from, to) = (side(self, from), side(self, to));
        let mut host = GameHost::new(self, None);
        match ferraloom_plugins::inventory_move_item(
            &mut host,
            world,
            from,
            to,
            source_initiated,
            item,
        ) {
            Some(moved) => {
                *item = moved;
                true
            }
            None => false,
        }
    }

    /// handlePlayerAction's PlayerSwapHandItemsEvent, before the hands swap:
    /// true when a plugin cancelled it (nothing swaps) or gave the hands other
    /// items (they get them here); false to swap as vanilla does.
    pub(in crate::game) fn plugin_swap_hands(&mut self, host: &mut LanHost, conn: u32) -> bool {
        let Some(g) = host.hub.guest(conn) else {
            return false;
        };
        let (id, slot) = (g.uuid, g.selected);
        // What each hand holds after the swap.
        let (main, off) = (g.offhand.clone(), g.inventory[slot].clone());
        let mut gh = GameHost::new(self, Some(host));
        let (main, off) =
            match ferraloom_plugins::player_swap_hands(&mut gh, id, main.as_ref(), off.as_ref()) {
                Swap::Unchanged => return false,
                Swap::Cancelled => return true,
                Swap::Changed(main, off) => (main, off),
            };
        let Some(g) = host.hub.guests.iter_mut().find(|g| g.conn == conn) else {
            return true;
        };
        let old_main = std::mem::replace(&mut g.inventory[slot], main.clone());
        let old_off = std::mem::replace(&mut g.offhand, off.clone());
        // setItemInHand: the off hand, then the main hand; the item in use stops.
        self.as_guest(host, conn, |g| {
            g.using = None;
            g.player_on_equip("offhand", old_off.as_ref(), off.as_ref());
            g.player_on_equip("mainhand", old_main.as_ref(), main.as_ref());
        });
        true
    }

    /// LivingEntity.completeUsingItem's PlayerItemConsumeEvent for the guest
    /// whose turn it is, before `stack` (held in `slot`) has any effect: true
    /// when a plugin cancelled it. Their client, which consumed it already,
    /// gets its inventory again (`updateInventory`), as CraftBukkit sends it.
    pub(in crate::game) fn plugin_consume(&mut self, slot: usize, stack: &ItemStack) -> bool {
        let Some(id) = self.lan_acting_uuid else {
            return false;
        };
        let off_hand = slot == crate::game::shield::OFFHAND_SLOT;
        let in_place = self.lan.is_some();
        ferraloom_plugins::borrow_lent(|lent: Option<&mut LanHost>| {
            let mut host = GameHost::new(self, lent.filter(|_| !in_place));
            let cancelled = ferraloom_plugins::player_item_consume(&mut host, id, stack, off_hand);
            if cancelled {
                host.resend_inventory(id);
            }
            cancelled
        })
    }
}
