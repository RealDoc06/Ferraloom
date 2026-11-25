package dev.ferraloom.bukkit;

import java.util.UUID;
import org.bukkit.event.Event;
import org.bukkit.event.inventory.ClickType;
import org.bukkit.event.inventory.InventoryAction;
import org.bukkit.event.inventory.InventoryClickEvent;
import org.bukkit.event.inventory.InventoryType.SlotType;

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
}
