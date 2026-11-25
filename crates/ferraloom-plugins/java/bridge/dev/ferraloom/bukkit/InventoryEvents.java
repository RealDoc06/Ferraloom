package dev.ferraloom.bukkit;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import org.bukkit.event.Event;
import org.bukkit.event.inventory.ClickType;
import org.bukkit.event.inventory.InventoryAction;
import org.bukkit.event.inventory.InventoryClickEvent;
import org.bukkit.event.inventory.InventoryDragEvent;
import org.bukkit.event.inventory.InventoryMoveItemEvent;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.event.inventory.InventoryType.SlotType;
import org.bukkit.event.player.PlayerItemConsumeEvent;
import org.bukkit.event.player.PlayerSwapHandItemsEvent;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/**
 * The inventory area's event entry points (crates/ferraloom-plugins/src/inventory.rs), on the
 * server thread. Menus are named as {@link InventoryNative} names inventories; items are read only
 * when a plugin asks the event for them.
 */
public final class InventoryEvents {
    private InventoryEvents() {
    }

    /**
     * ServerGamePacketListenerImpl.handleContainerClick's InventoryClickEvent, before the click:
     * true when it must not happen (cancelled, denied, or the plugin opened or closed a menu).
     */
    public static boolean click(long most, long least, int kind, long a, long b, String topType, int rawSlot,
            String click, String action, int hotbarKey) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return false;
        }
        try {
            FerraloomInventoryView view = player.inventories.view(kind, a, b, topType);
            SlotType slotType = view.getSlotType(rawSlot);
            ClickType type = ClickType.valueOf(click);
            InventoryAction act = InventoryAction.valueOf(action);
            InventoryClickEvent event = type == ClickType.NUMBER_KEY
                    ? new InventoryClickEvent(view, slotType, rawSlot, type, act, hotbarKey)
                    : new InventoryClickEvent(view, slotType, rawSlot, type, act);
            Bridge.server.pluginManager.callEvent(event);
            // SPIGOT-1224: a plugin that opened or closed a menu ends the click.
            boolean moved = kind == 2 ? player.inventories.open() != null : player.inventories.open() != view;
            return event.getResult() == Event.Result.DENY || moved;
        } catch (Throwable ex) {
            Bridge.log(ex, "an inventory click");
            return false;
        }
    }

    /**
     * AbstractContainerMenu.doClick's InventoryDragEvent as a drag ends, before anything lands:
     * true when denied. {@code slots[i]} would hold {@code items[i]}.
     */
    public static boolean drag(long most, long least, int kind, long a, long b, String topType, int[] slots,
            byte[][] items, byte[] newCursor, byte[] oldCursor, boolean right) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return false;
        }
        try {
            FerraloomInventoryView view = player.inventories.view(kind, a, b, topType);
            Map<Integer, ItemStack> placed = new HashMap<>();
            for (int i = 0; i < slots.length; i++) {
                placed.put(slots[i], Items.read(items[i]));
            }
            InventoryDragEvent event = new InventoryDragEvent(view, Items.read(newCursor),
                    Items.orAir(Items.read(oldCursor)), right, placed);
            Bridge.server.pluginManager.callEvent(event);
            boolean moved = kind == 2 ? player.inventories.open() != null : player.inventories.open() != view;
            return event.getResult() == Event.Result.DENY || moved;
        } catch (Throwable ex) {
            Bridge.log(ex, "an inventory drag");
            return false;
        }
    }

    /** ServerPlayer.openMenu's InventoryOpenEvent for a menu the game opens: true when cancelled. */
    public static boolean opened(long most, long least, int kind, long a, long b, String topType) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return false;
        }
        try {
            return player.inventories.opening(kind, a, b, topType);
        } catch (Throwable ex) {
            Bridge.log(ex, "an inventory opening");
            return false;
        }
    }

    /** handleContainerClose's InventoryCloseEvent, as the client closes its menu. */
    public static void closed(long most, long least, int kind, long a, long b, String topType) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return;
        }
        try {
            player.inventories.closed(kind, a, b, topType);
        } catch (Throwable ex) {
            Bridge.log(ex, "an inventory closing");
        }
    }

    /**
     * HopperBlockEntity's InventoryMoveItemEvent, for one item moving between two block containers
     * of {@code world} (positions as BlockPos.asLong): the item to move, or null when cancelled.
     */
    public static byte[] moveItem(int world, long from, String fromType, long to, String toType, boolean sourceInitiated,
            byte[] item) {
        try {
            FerraloomInventory source = new FerraloomInventory(4, world, from, InventoryType.valueOf(fromType), -1, null, null);
            FerraloomInventory destination = new FerraloomInventory(4, world, to, InventoryType.valueOf(toType), -1, null, null);
            ItemStack stack = Items.read(item);
            InventoryMoveItemEvent event = new InventoryMoveItemEvent(source, stack, destination, sourceInitiated);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled() ? null : Items.write(event.getItem());
        } catch (Throwable ex) {
            Bridge.log(ex, "an item moving between inventories");
            return item;
        }
    }

    /** LivingEntity.completeUsingItem's PlayerItemConsumeEvent, before the item has any effect: true when cancelled. */
    public static boolean consume(long most, long least, byte[] item, boolean offHand) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return false;
        }
        try {
            PlayerItemConsumeEvent event = new PlayerItemConsumeEvent(player, Items.orAir(Items.read(item)),
                    offHand ? EquipmentSlot.OFF_HAND : EquipmentSlot.HAND);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled();
        } catch (Throwable ex) {
            Bridge.log(ex, "an item being consumed");
            return false;
        }
    }

    /**
     * handlePlayerAction's PlayerSwapHandItemsEvent, {@code main} and {@code off} what each hand
     * would hold: null when cancelled, nothing when the plugin left the items, else {main, off}.
     */
    public static byte[][] swapHands(long most, long least, byte[] main, byte[] off) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return new byte[0][];
        }
        try {
            ItemStack mainHand = Items.orAir(Items.read(main));
            ItemStack offHand = Items.orAir(Items.read(off));
            PlayerSwapHandItemsEvent event = new PlayerSwapHandItemsEvent(player, mainHand.clone(), offHand.clone());
            Bridge.server.pluginManager.callEvent(event);
            if (event.isCancelled()) {
                return null;
            }
            if (mainHand.equals(event.getMainHandItem()) && offHand.equals(event.getOffHandItem())) {
                return new byte[0][];
            }
            return new byte[][] {Items.write(event.getMainHandItem()), Items.write(event.getOffHandItem())};
        } catch (Throwable ex) {
            Bridge.log(ex, "a player swapping hands");
            return new byte[0][];
        }
    }
}
