//! The inventory area: item stacks across JNI, inventories and their events
//! (`java/bridge/.../InventoryEvents.java`, `InventoryNative.java`).
//!
//! An item crosses as one `byte[]`: `ItemStack.CODEC`'s compound (`id`,
//! `count`, `components`) in network NBT, null for an empty slot. Java reads
//! it into a Bukkit `ItemStack` whose meta keeps the component patch as NBT,
//! so components neither side models come back as they went
//! (`Items.java`). Converting allocates on both sides: event calls pass
//! items only when the event needs them, and the Bukkit API fetches the rest
//! when a plugin asks.
use crate::{
    PlayerId,
    bridge::{self, Entry, obj},
    natives::{self, Deferred, Fail, guard, jbool, player_id, string, with_host},
};
use horizon_items::{item_nbt, items::ItemStack};
use horizon_world::nbt::{self, Limits, Tag};
use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JObject, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jbyteArray, jint, jintArray, jlong, jstring},
};
use std::{ffi::c_void, sync::OnceLock};

/// Where a Bukkit inventory's slots are. Java names one by `(kind, a, b)`
/// (`InventoryNative.java`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// A player's `PlayerInventory`, in Bukkit's slots: 0-8 the hotbar,
    /// 9-35 the rest, 36-39 boots, leggings, chestplate, helmet, 40 the off hand.
    Player(PlayerId),
    /// A player's ender chest (27 slots).
    Ender(PlayerId),
    /// A player's 2x2 crafting grid: 0 the result, 1-4 the grid.
    Crafting(PlayerId),
    /// A plugin's container (`Server.createInventory`), by its id.
    Custom(u32),
    /// The container slots of the menu a player has open (a chest, a double
    /// chest, a furnace...), without their inventory's.
    Menu(PlayerId),
    /// A block entity's container.
    Block { world: usize, pos: [i32; 3] },
}

impl Holder {
    fn from_java(kind: jint, a: jlong, b: jlong) -> Option<Self> {
        Some(match kind {
            0 => Holder::Player(player_id(a, b)),
            1 => Holder::Ender(player_id(a, b)),
            2 => Holder::Crafting(player_id(a, b)),
            3 => Holder::Custom(u32::try_from(a).ok()?),
            4 => Holder::Block {
                world: usize::try_from(a).ok()?,
                pos: unpack(b),
            },
            5 => Holder::Menu(player_id(a, b)),
            _ => return None,
        })
    }

    fn to_java(self) -> (jint, jlong, jlong) {
        let player = |kind, id| {
            let (most, least) = natives::halves(id);
            (kind, most, least)
        };
        match self {
            Holder::Player(id) => player(0, id),
            Holder::Ender(id) => player(1, id),
            Holder::Crafting(id) => player(2, id),
            Holder::Custom(id) => (3, jlong::from(id), 0),
            Holder::Block { world, pos } => (4, world as jlong, pack(pos)),
            Holder::Menu(id) => player(5, id),
        }
    }
}

/// `BlockPos.asLong`: x and z in 26 bits, y in 12.
fn pack([x, y, z]: [i32; 3]) -> jlong {
    ((i64::from(x) & 0x3FF_FFFF) << 38)
        | ((i64::from(z) & 0x3FF_FFFF) << 12)
        | (i64::from(y) & 0xFFF)
}

fn unpack(v: jlong) -> [i32; 3] {
    [
        (v >> 38) as i32,
        ((v << 52) >> 52) as i32,
        ((v << 26) >> 38) as i32,
    ]
}

/// What `Inventory`, `PlayerInventory` and `Player.openInventory` read and
/// change of the game. Every method has a default (nothing there), so a
/// test host need not implement the area.
pub trait InventoryHost {
    /// How many slots the holder has; none when it is gone (a player who
    /// left, a block broken, a plugin's container freed).
    fn inventory_size(&mut self, _at: Holder) -> Option<usize> {
        None
    }
    fn slot(&mut self, _at: Holder, _slot: usize) -> Option<ItemStack> {
        None
    }
    /// Sets a slot; false when there is no such slot. The players who see
    /// it are told at the end of the tick, as the game tells them of its own changes.
    fn set_slot(&mut self, _at: Holder, _slot: usize, _item: Option<ItemStack>) -> bool {
        false
    }
    /// The hotbar slot a player holds (0-8).
    fn held_slot(&mut self, _player: PlayerId) -> Option<usize> {
        None
    }
    fn set_held_slot(&mut self, _player: PlayerId, _slot: usize) -> bool {
        false
    }
    /// What a player carries on the cursor of an open menu.
    fn cursor(&mut self, _player: PlayerId) -> Option<ItemStack> {
        None
    }
    fn set_cursor(&mut self, _player: PlayerId, _item: Option<ItemStack>) -> bool {
        false
    }
    /// A new plugin container of `size` slots: its id.
    fn create_container(&mut self, _size: usize) -> u32 {
        0
    }
    fn drop_container(&mut self, _id: u32) {}
    /// Shows a container to a player (`ServerPlayer.openMenu`): `menu` a
    /// `minecraft:menu` id, `title` a text component. False when it could not.
    fn open_container(&mut self, _player: PlayerId, _at: Holder, _menu: &str, _title: Tag) -> bool {
        false
    }
    /// Closes the player's open menu (`closeContainer`); false when none was open.
    fn close_container(&mut self, _player: PlayerId) -> bool {
        false
    }
    /// Sends the player's whole inventory and open menu again (`Player.updateInventory`).
    fn resend_inventory(&mut self, _player: PlayerId) {}
    /// What `ItemType` answers of an item (`minecraft:stone`); none for an unknown one.
    fn item_facts(&mut self, _item: &str) -> Option<ItemFacts> {
        None
    }
}

/// An item's prototype, as `ItemType` reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemFacts {
    pub max_stack: u32,
    pub max_damage: u32,
    pub edible: bool,
    pub fuel: bool,
    pub record: bool,
    /// A composter takes it.
    pub compostable: bool,
    /// `Item.getCraftingRemainder`'s item id.
    pub remainder: Option<String>,
}

/// An `InventoryClickEvent` about to happen: the click as CraftBukkit reads
/// it (`ServerGamePacketListenerImpl.handleContainerClick`).
#[derive(Clone, Copy, Debug)]
pub struct Click<'a> {
    pub player: PlayerId,
    /// The menu's own slots (none for the player's inventory menu, whose top is
    /// the crafting grid).
    pub top: Holder,
    /// The `InventoryType` name of the top: `CHEST`, `HOPPER`, `CRAFTING`...
    pub top_type: &'a str,
    pub raw_slot: i32,
    /// The `ClickType` name: `LEFT`, `SHIFT_LEFT`, `NUMBER_KEY`...
    pub click: &'a str,
    /// The `InventoryAction` name: `PICKUP_ALL`, `MOVE_TO_OTHER_INVENTORY`...
    pub action: &'a str,
    /// The hotbar key of a `NUMBER_KEY` click, else -1.
    pub hotbar_key: i32,
}

const S: &str = "Ljava/lang/String;";
const CLICK: usize = 0;
const OPENED: usize = 1;
const CLOSED: usize = 2;
const MOVE_ITEM: usize = 3;
const DRAG: usize = 4;

static EVENTS: OnceLock<Entry<5>> = OnceLock::new();

/// At start: `InventoryNative`'s methods bound, `InventoryEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    natives::register(
        env,
        "dev/ferraloom/bukkit/InventoryNative",
        &[
            ("size", "(IJJ)I", size as *mut c_void),
            ("slot", "(IJJI)[B", slot as *mut c_void),
            ("setSlot", "(IJJI[B)Z", set_slot as *mut c_void),
            ("replaceSlot", "(IJJI[B[B)Z", replace_slot as *mut c_void),
            ("heldSlot", "(JJ)I", held_slot as *mut c_void),
            ("setHeldSlot", "(JJI)Z", set_held_slot as *mut c_void),
            ("cursor", "(JJ)[B", cursor as *mut c_void),
            ("setCursor", "(JJ[B)Z", set_cursor as *mut c_void),
            ("createContainer", "(I)I", create_container as *mut c_void),
            ("dropContainer", "(I)V", drop_container as *mut c_void),
            (
                "open",
                "(JJIJJLjava/lang/String;[B)Z",
                open_container as *mut c_void,
            ),
            ("close", "(JJ)Z", close_container as *mut c_void),
            ("resend", "(JJ)V", resend as *mut c_void),
            (
                "itemFacts",
                "(Ljava/lang/String;)[I",
                item_facts as *mut c_void,
            ),
            (
                "itemRemainder",
                "(Ljava/lang/String;)Ljava/lang/String;",
                item_remainder as *mut c_void,
            ),
        ],
    )?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/InventoryEvents",
        [
            ("click", format!("(JJIJJ{S}I{S}{S}I)Z")),
            ("opened", format!("(JJIJJ{S})Z")),
            ("closed", format!("(JJIJJ{S})V")),
            ("moveItem", format!("(IJ{S}J{S}Z[B)[B")),
            ("drag", format!("(JJIJJ{S}[I[[B[B[BZ)Z")),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// `InventoryClickEvent` (or `CraftItemEvent`), before the click changes
/// anything: true when a plugin cancelled it (or denied its result), and
/// the click must not happen. The items are not passed: the event's
/// getters read them from the menu when a plugin asks.
pub fn inventory_click(host: &mut dyn crate::PluginHost, c: &Click) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    let (most, least) = natives::halves(c.player);
    let (kind, a, b) = c.top.to_java();
    bridge::call(host, |env| {
        let top_type = env.new_string(c.top_type)?;
        let click = env.new_string(c.click)?;
        let action = env.new_string(c.action)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            JValue::Int(kind).as_jni(),
            JValue::Long(a).as_jni(),
            JValue::Long(b).as_jni(),
            obj(&top_type),
            JValue::Int(c.raw_slot).as_jni(),
            obj(&click),
            obj(&action),
            JValue::Int(c.hotbar_key).as_jni(),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `click`'s signature in `load`.
        unsafe { events.invoke(env, CLICK, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// `InventoryOpenEvent` for a menu the game is about to open for a player
/// (a chest they used): true when a plugin cancelled it, and it must not open.
pub fn inventory_open(
    host: &mut dyn crate::PluginHost,
    player: PlayerId,
    top: Holder,
    top_type: &str,
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    let (most, least) = natives::halves(player);
    let (kind, a, b) = top.to_java();
    bridge::call(host, |env| {
        let top_type = env.new_string(top_type)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            JValue::Int(kind).as_jni(),
            JValue::Long(a).as_jni(),
            JValue::Long(b).as_jni(),
            obj(&top_type),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `opened`'s signature in `load`.
        unsafe { events.invoke(env, OPENED, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// `InventoryCloseEvent` for the player's open menu (`top`, of
/// `InventoryType` `top_type`), as the client closes it.
pub fn inventory_close(
    host: &mut dyn crate::PluginHost,
    player: PlayerId,
    top: Holder,
    top_type: &str,
) {
    let Some(events) = EVENTS.get() else { return };
    let (most, least) = natives::halves(player);
    let (kind, a, b) = top.to_java();
    bridge::call(host, |env| {
        let top_type = env.new_string(top_type)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            JValue::Int(kind).as_jni(),
            JValue::Long(a).as_jni(),
            JValue::Long(b).as_jni(),
            obj(&top_type),
        ];
        let void = ReturnType::Primitive(Primitive::Void);
        // SAFETY: `closed`'s signature in `load`.
        unsafe { events.invoke(env, CLOSED, void, &args) }?;
        Ok(())
    });
}

/// An `InventoryDragEvent` about to happen: the drag's last packet, before
/// the items land (`AbstractContainerMenu.doClick`'s QUICK_CRAFT end).
#[derive(Clone, Copy, Debug)]
pub struct Drag<'a> {
    pub player: PlayerId,
    pub top: Holder,
    pub top_type: &'a str,
    /// Each raw slot dragged over, with the stack it would hold.
    pub slots: &'a [(i32, ItemStack)],
    /// The cursor after the drag; none when it empties.
    pub new_cursor: Option<&'a ItemStack>,
    pub old_cursor: &'a ItemStack,
    /// A right-button drag (one item per slot).
    pub right: bool,
}

/// `InventoryDragEvent`: true when a plugin denied it, and nothing may land.
pub fn inventory_drag(host: &mut dyn crate::PluginHost, d: &Drag) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    let (most, least) = natives::halves(d.player);
    let (kind, a, b) = d.top.to_java();
    bridge::call(host, |env| {
        let top_type = env.new_string(d.top_type)?;
        let raw: Vec<jint> = d.slots.iter().map(|(s, _)| *s).collect();
        let slots = env.new_int_array(raw.len() as jint)?;
        env.set_int_array_region(&slots, 0, &raw)?;
        let items = env.new_object_array(raw.len() as jint, "[B", JObject::null())?;
        for (i, (_, item)) in d.slots.iter().enumerate() {
            let bytes = to_java(env, Some(item))?;
            env.set_object_array_element(&items, i as jint, bytes)?;
        }
        let new_cursor = to_java(env, d.new_cursor)?;
        let old_cursor = to_java(env, Some(d.old_cursor))?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            JValue::Int(kind).as_jni(),
            JValue::Long(a).as_jni(),
            JValue::Long(b).as_jni(),
            obj(&top_type),
            obj(&slots),
            obj(&items),
            obj(&new_cursor),
            obj(&old_cursor),
            JValue::Bool(d.right.into()).as_jni(),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `drag`'s signature in `load`.
        unsafe { events.invoke(env, DRAG, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// A block container's side of an `InventoryMoveItemEvent`: where it is
/// and its `InventoryType` name (`HOPPER`, `CHEST`...).
#[derive(Clone, Copy, Debug)]
pub struct Side<'a> {
    pub pos: [i32; 3],
    pub kind: &'a str,
}

/// `InventoryMoveItemEvent` as a hopper moves `item` (one item) from one
/// block container to another in `world`, before it lands: the item to move
/// (the plugin may have changed it), or none when a plugin cancelled the
/// move, which must then put the item back. `source_initiated`: the source
/// pushed it (a hopper ejecting) rather than the destination pulling.
/// Without a listener this is never called; with one, each transfer costs
/// one JNI call, the item's NBT both ways, and the event's objects.
pub fn inventory_move_item(
    host: &mut dyn crate::PluginHost,
    world: usize,
    from: Side,
    to: Side,
    source_initiated: bool,
    item: &ItemStack,
) -> Option<ItemStack> {
    let Some(events) = EVENTS.get() else {
        return Some(item.clone());
    };
    let out = bridge::call(host, |env| {
        let from_kind = env.new_string(from.kind)?;
        let to_kind = env.new_string(to.kind)?;
        let bytes = to_java(env, Some(item))?;
        let args = [
            JValue::Int(world as jint).as_jni(),
            JValue::Long(pack(from.pos)).as_jni(),
            obj(&from_kind),
            JValue::Long(pack(to.pos)).as_jni(),
            obj(&to_kind),
            JValue::Bool(source_initiated.into()).as_jni(),
            obj(&bytes),
        ];
        // SAFETY: `moveItem`'s signature in `load`.
        let moved = unsafe { events.invoke(env, MOVE_ITEM, ReturnType::Array, &args) }?.l()?;
        let moved = JByteArray::from(moved);
        Ok(if moved.is_null() {
            None
        } else {
            decode(&env.convert_byte_array(&moved)?)
        })
    });
    // A bridge that failed lets the move happen, as an event no one cancelled.
    out.unwrap_or_else(|| Some(item.clone()))
}

fn holder(kind: jint, a: jlong, b: jlong) -> Result<Holder, Fail> {
    // Java names only these: anything else is a bug there, answered as nothing.
    Holder::from_java(kind, a, b).ok_or(Fail::Jni)
}

fn index(slot: jint) -> usize {
    usize::try_from(slot).unwrap_or(usize::MAX)
}

extern "system" fn size(mut env: JNIEnv, _: JClass, kind: jint, a: jlong, b: jlong) -> jint {
    guard(&mut env, -1, |_| {
        let at = holder(kind, a, b)?;
        let n = with_host(|h| h.inventory_size(at)).ok_or(Fail::NoHost("Inventory.getSize"))?;
        Ok(n.map_or(-1, |n| n as jint))
    })
}

extern "system" fn slot(
    mut env: JNIEnv,
    _: JClass,
    kind: jint,
    a: jlong,
    b: jlong,
    slot: jint,
) -> jbyteArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let at = holder(kind, a, b)?;
        let item =
            with_host(|h| h.slot(at, index(slot))).ok_or(Fail::NoHost("Inventory.getItem"))?;
        Ok(to_java(env, item.as_ref())?.into_raw())
    })
}

extern "system" fn set_slot(
    mut env: JNIEnv,
    _: JClass,
    kind: jint,
    a: jlong,
    b: jlong,
    slot: jint,
    item: JByteArray,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let at = holder(kind, a, b)?;
        let item = from_java(env, &item)?;
        let set = with_host(|h| h.set_slot(at, index(slot), item))
            .ok_or(Fail::NoHost("Inventory.setItem"))?;
        Ok(jbool(set))
    })
}

#[allow(clippy::too_many_arguments)]
/// A slot set only while it still holds `expected`: an item a plugin got
/// from an inventory writes through to its slot as CraftItemStack's mirror
/// does, and stops once the slot holds another stack.
extern "system" fn replace_slot(
    mut env: JNIEnv,
    _: JClass,
    kind: jint,
    a: jlong,
    b: jlong,
    slot: jint,
    expected: JByteArray,
    item: JByteArray,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let at = holder(kind, a, b)?;
        let (expected, item) = (from_java(env, &expected)?, from_java(env, &item)?);
        let set = with_host(|h| {
            let slot = index(slot);
            h.slot(at, slot) == expected && h.set_slot(at, slot, item)
        })
        .ok_or(Fail::NoHost("ItemStack.setAmount"))?;
        Ok(jbool(set))
    })
}

extern "system" fn held_slot(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jint {
    guard(&mut env, 0, |_| {
        let held = with_host(|h| h.held_slot(player_id(most, least)))
            .ok_or(Fail::NoHost("PlayerInventory.getHeldItemSlot"))?;
        Ok(held.map_or(0, |s| s as jint))
    })
}

extern "system" fn set_held_slot(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    slot: jint,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let set = with_host(|h| h.set_held_slot(player_id(most, least), index(slot)))
            .ok_or(Fail::NoHost("PlayerInventory.setHeldItemSlot"))?;
        Ok(jbool(set))
    })
}

extern "system" fn cursor(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jbyteArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let item = with_host(|h| h.cursor(player_id(most, least)))
            .ok_or(Fail::NoHost("HumanEntity.getItemOnCursor"))?;
        Ok(to_java(env, item.as_ref())?.into_raw())
    })
}

extern "system" fn set_cursor(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    item: JByteArray,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let item = from_java(env, &item)?;
        let set = with_host(|h| h.set_cursor(player_id(most, least), item))
            .ok_or(Fail::NoHost("HumanEntity.setItemOnCursor"))?;
        Ok(jbool(set))
    })
}

extern "system" fn create_container(mut env: JNIEnv, _: JClass, size: jint) -> jint {
    guard(&mut env, 0, |_| {
        let id = with_host(|h| h.create_container(index(size)))
            .ok_or(Fail::NoHost("Server.createInventory"))?;
        Ok(id as jint)
    })
}

/// From any thread: the collector frees a plugin's container when its
/// Java object goes, so off the server thread it waits for the next tick.
extern "system" fn drop_container(mut env: JNIEnv, _: JClass, id: jint) {
    guard(&mut env, (), |_| {
        let id = id as u32;
        if with_host(|h| h.drop_container(id)).is_none() {
            natives::defer(Deferred::DropContainer(id));
        }
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn open_container(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    kind: jint,
    a: jlong,
    b: jlong,
    menu: JString,
    title: JByteArray,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let at = holder(kind, a, b)?;
        let menu = string(env, &menu)?;
        let title = env.convert_byte_array(&title)?;
        let title = nbt::read_network(&title, Limits::default())
            .map(|(t, _)| t)
            .unwrap_or_else(|_| Tag::String("".into()));
        let opened = with_host(|h| h.open_container(player_id(most, least), at, &menu, title))
            .ok_or(Fail::NoHost("Player.openInventory"))?;
        Ok(jbool(opened))
    })
}

extern "system" fn close_container(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let closed = with_host(|h| h.close_container(player_id(most, least)))
            .ok_or(Fail::NoHost("HumanEntity.closeInventory"))?;
        Ok(jbool(closed))
    })
}

/// {max stack, max damage, edible, fuel, record, compostable}, or null.
extern "system" fn item_facts(mut env: JNIEnv, _: JClass, item: JString) -> jintArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let item = string(env, &item)?;
        let facts =
            with_host(|h| h.item_facts(&item)).ok_or(Fail::NoHost("ItemType.getMaxStackSize"))?;
        let Some(f) = facts else {
            return Ok(std::ptr::null_mut());
        };
        let values = [
            f.max_stack as jint,
            f.max_damage as jint,
            jint::from(f.edible),
            jint::from(f.fuel),
            jint::from(f.record),
            jint::from(f.compostable),
        ];
        let array = env.new_int_array(values.len() as jint)?;
        env.set_int_array_region(&array, 0, &values)?;
        Ok(array.into_raw())
    })
}

extern "system" fn item_remainder(mut env: JNIEnv, _: JClass, item: JString) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let item = string(env, &item)?;
        let facts = with_host(|h| h.item_facts(&item))
            .ok_or(Fail::NoHost("ItemType.getCraftingRemainingItem"))?;
        match facts.and_then(|f| f.remainder) {
            Some(r) => Ok(env.new_string(r)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

extern "system" fn resend(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) {
    guard(&mut env, (), |_| {
        with_host(|h| h.resend_inventory(player_id(most, least)))
            .ok_or(Fail::NoHost("Player.updateInventory"))?;
        Ok(())
    })
}

/// A stack as Java's `Items.read` takes it; empty for none.
pub fn encode(stack: Option<&ItemStack>) -> Vec<u8> {
    let Some(stack) = stack.filter(|s| s.count > 0) else {
        return Vec::new();
    };
    // An item's compound always encodes: the limits are the default's.
    nbt::write_network(&item_nbt::item_to_nbt(stack), Limits::default()).unwrap_or_default()
}

/// What Java's `Items.write` gave; none for empty bytes, air, a zero count
/// or bytes that are not an item.
pub fn decode(bytes: &[u8]) -> Option<ItemStack> {
    if bytes.is_empty() {
        return None;
    }
    let (tag, _) = nbt::read_network(bytes, Limits::default()).ok()?;
    let count = tag.get("count").and_then(|c| match c {
        nbt::Tag::Byte(v) => Some(i32::from(*v)),
        nbt::Tag::Short(v) => Some(i32::from(*v)),
        nbt::Tag::Int(v) => Some(*v),
        _ => None,
    });
    if count.is_some_and(|c| c <= 0) {
        return None;
    }
    item_nbt::item_from_nbt(&tag)
}

/// A stack as a Java `byte[]`, or null for none.
pub(crate) fn to_java<'l>(
    env: &mut JNIEnv<'l>,
    stack: Option<&ItemStack>,
) -> jni::errors::Result<JObject<'l>> {
    let bytes = encode(stack);
    if bytes.is_empty() {
        return Ok(JObject::null());
    }
    Ok(env.byte_array_from_slice(&bytes)?.into())
}

/// A Java `byte[]` (or null) as a stack.
pub(crate) fn from_java(env: &JNIEnv, bytes: &JByteArray) -> Result<Option<ItemStack>, Fail> {
    if bytes.is_null() {
        return Ok(None);
    }
    Ok(decode(&env.convert_byte_array(bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::nbt::Tag;

    /// A sword as a plugin leaves it: enchanted, damaged and named (modelled
    /// by Horizon), with lore, plugin data and a removed component (not).
    fn plugin_sword() -> ItemStack {
        let components = Tag::compound([
            ("minecraft:damage", Tag::Int(12)),
            (
                "minecraft:enchantments",
                Tag::compound([("minecraft:sharpness", Tag::Int(5))]),
            ),
            ("minecraft:custom_name", Tag::String("Blade".into())),
            (
                "minecraft:lore",
                Tag::List(vec![Tag::compound([
                    ("text", Tag::String("Sharp".into())),
                    ("color", Tag::String("gold".into())),
                    ("italic", Tag::Byte(0)),
                ])]),
            ),
            (
                "minecraft:custom_data",
                Tag::compound([(
                    "PublicBukkitValues",
                    Tag::compound([("shop:price", Tag::Long(250))]),
                )]),
            ),
            ("!minecraft:tool", Tag::Compound(Default::default())),
        ]);
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:diamond_sword".into())),
            ("count", Tag::Int(1)),
            ("components", components),
        ]);
        item_nbt::item_from_nbt(&tag).unwrap()
    }

    #[test]
    fn a_stack_survives_the_round_trip_with_the_components_it_does_not_model() {
        let sword = plugin_sword();
        let back = decode(&encode(Some(&sword))).unwrap();
        assert_eq!(back, sword);
        assert_eq!(back.damage, 12);
        assert_eq!(back.enchantment_level("minecraft:sharpness"), 5);
        let data = back.data().unwrap();
        assert!(data.other("minecraft:lore").is_some());
        assert!(data.other("!minecraft:tool").is_some());
        let price = data
            .other("minecraft:custom_data")
            .and_then(|d| d.get("PublicBukkitValues"))
            .and_then(|v| v.get("shop:price"));
        assert_eq!(price, Some(&Tag::Long(250)));
    }

    #[test]
    fn holders_cross_as_java_names_them() {
        for at in [
            Holder::Player(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210),
            Holder::Ender(7),
            Holder::Crafting(9),
            Holder::Custom(42),
            Holder::Menu(3),
            Holder::Block {
                world: 2,
                pos: [-30_000_000, -64, 29_999_999],
            },
            Holder::Block {
                world: 0,
                pos: [5, 319, -1],
            },
        ] {
            let (kind, a, b) = at.to_java();
            assert_eq!(Holder::from_java(kind, a, b), Some(at));
        }
        assert_eq!(Holder::from_java(9, 0, 0), None);
    }

    #[test]
    fn empty_stacks_cross_as_nothing() {
        assert!(encode(None).is_empty());
        assert!(encode(Some(&ItemStack::new("minecraft:stone", 0))).is_empty());
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[1, 2, 3]), None);
        let air = Tag::compound([
            ("id", Tag::String("minecraft:air".into())),
            ("count", Tag::Int(1)),
        ]);
        assert_eq!(
            decode(&nbt::write_network(&air, Limits::default()).unwrap()),
            None
        );
        // Bukkit lets a plugin set an amount of zero: that slot is empty.
        let none = Tag::compound([
            ("id", Tag::String("minecraft:stone".into())),
            ("count", Tag::Int(0)),
        ]);
        assert_eq!(
            decode(&nbt::write_network(&none, Limits::default()).unwrap()),
            None
        );
        let stone = decode(&encode(Some(&ItemStack::new("minecraft:stone", 64)))).unwrap();
        assert_eq!(
            (&*stone.item, stone.count, stone.extra),
            ("minecraft:stone", 64, None)
        );
    }
}
