package dev.ferraloom.bukkit;

import com.google.common.base.Preconditions;
import java.lang.ref.Cleaner;
import java.lang.ref.WeakReference;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import org.bukkit.event.inventory.InventoryCloseEvent;
import org.bukkit.event.inventory.InventoryOpenEvent;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/**
 * Plugins' containers ({@code Server.createInventory}) and each player's inventories and open
 * menu ({@link Of}). A plugin's container lives in the server (src/inventory.rs, Holder::Custom)
 * as long as its Java object does: the collector frees it.
 */
final class FerraloomInventories {
    /** Plugins' containers by id, while their object lives: events name them by id. */
    private static final Map<Integer, WeakReference<FerraloomInventory>> CUSTOM = new ConcurrentHashMap<>();
    private static final Cleaner CLEANER = Cleaner.create();

    private FerraloomInventories() {
    }

    /** CraftServer.createInventory(holder, size, title): a chest of 9 to 54 slots. */
    static Inventory create(InventoryHolder holder, int size, String title) {
        Preconditions.checkArgument(size >= 9 && size <= 54 && size % 9 == 0,
                "Size for custom inventory must be a multiple of 9 between 9 and 54 slots (got %s)", size);
        return create(holder, InventoryType.CHEST, size, title);
    }

    /** CraftServer.createInventory(holder, type, title), for the types a container menu shows. */
    static Inventory create(InventoryHolder holder, InventoryType type, String title) {
        Preconditions.checkArgument(type != null, "InventoryType cannot be null");
        return switch (type) {
            case CHEST, ENDER_CHEST, BARREL, DISPENSER, DROPPER, HOPPER, SHULKER_BOX ->
                create(holder, type, type.getDefaultSize(), title);
            default -> throw new UnsupportedOperationException("Ferraloom does not implement Server.createInventory("
                    + type + ") yet");
        };
    }

    private static FerraloomInventory create(InventoryHolder holder, InventoryType type, int size, String title) {
        int id = InventoryNative.createContainer(size);
        FerraloomInventory inventory = new FerraloomInventory(3, id, 0, type, size, holder, title);
        CUSTOM.put(id, new WeakReference<>(inventory));
        CLEANER.register(inventory, () -> {
            CUSTOM.remove(id);
            InventoryNative.dropContainer(id);
        });
        return inventory;
    }

    /** A title as the open-screen packet carries it: CraftChatMessage's component of the legacy text. */
    static byte[] title(String legacy) {
        return Nbt.write(Texts.fromLegacy(legacy == null ? "" : legacy));
    }

    /** A player's inventories and the menu they have open. */
    static final class Of {
        private final FerraloomPlayer player;
        private FerraloomPlayerInventory inventory;
        private FerraloomInventory enderChest;
        private FerraloomInventoryView own;
        /** The menu open over the player's inventory, null for none (their own inventory). */
        private FerraloomInventoryView open;

        Of(FerraloomPlayer player) {
            this.player = player;
        }

        private long most() {
            return player.id.getMostSignificantBits();
        }

        private long least() {
            return player.id.getLeastSignificantBits();
        }

        FerraloomPlayerInventory inventory() {
            if (inventory == null) {
                inventory = new FerraloomPlayerInventory(player);
            }
            return inventory;
        }

        FerraloomInventory enderChest() {
            if (enderChest == null) {
                enderChest = new FerraloomInventory(1, most(), least(), InventoryType.ENDER_CHEST, 27, player, null);
            }
            return enderChest;
        }

        /** The player's own inventory menu: the 2x2 crafting grid over their inventory. */
        FerraloomInventoryView own() {
            if (own == null) {
                own = new FerraloomInventoryView(player,
                        new FerraloomInventory(2, most(), least(), InventoryType.CRAFTING, 5, player, null));
            }
            return own;
        }

        FerraloomInventoryView open() {
            return open;
        }

        /** HumanEntity.getOpenInventory: the open menu, or the player's own. */
        InventoryView openView() {
            return open != null ? open : own();
        }

        /**
         * The view of the menu the server names ({@code (kind, a, b)} and its InventoryType), the
         * one already known when it is that one: a plugin's container is the plugin's own object.
         */
        FerraloomInventoryView view(int kind, long a, long b, String type) {
            if (kind == 2) {
                return own();
            }
            if (open != null && open.top.is(kind, a, b)) {
                return open;
            }
            open = new FerraloomInventoryView(player, top(kind, a, b, type));
            return open;
        }

        private FerraloomInventory top(int kind, long a, long b, String type) {
            if (kind == 1) {
                return enderChest();
            }
            if (kind == 3) {
                WeakReference<FerraloomInventory> ref = CUSTOM.get((int) a);
                FerraloomInventory custom = ref == null ? null : ref.get();
                if (custom != null) {
                    return custom;
                }
            }
            InventoryType t;
            try {
                t = InventoryType.valueOf(type);
            } catch (IllegalArgumentException e) {
                t = InventoryType.CHEST;
            }
            return new FerraloomInventory(kind, a, b, t, -1, null, null);
        }

        /** CraftHumanEntity.openInventory: the open menu closes (its close event first), then InventoryOpenEvent. */
        InventoryView openInventory(Inventory inventory) {
            if (!(inventory instanceof FerraloomInventory top) || !player.online) {
                return null;
            }
            if (top == own().top || top == inventory()) {
                return openView();
            }
            String menu = top.menu();
            if (menu == null) {
                throw new UnsupportedOperationException("Ferraloom does not implement opening a " + top.getType()
                        + " inventory yet");
            }
            closeOpen();
            FerraloomInventoryView view = new FerraloomInventoryView(player, top);
            InventoryOpenEvent event = new InventoryOpenEvent(view);
            Bridge.server.pluginManager.callEvent(event);
            if (event.isCancelled()) {
                return null;
            }
            if (!InventoryNative.open(most(), least(), top.kind, top.a, top.b, menu, title(view.getTitle()))) {
                return null;
            }
            open = view;
            return view;
        }

        /** The menu opens again, under its view's title (InventoryView.setTitle). */
        void reopen(FerraloomInventoryView view) {
            String menu = view.top.menu();
            if (menu != null) {
                InventoryNative.open(most(), least(), view.top.kind, view.top.a, view.top.b, menu, title(view.getTitle()));
            }
        }

        /** InventoryCloseEvent for the open menu, which is then forgotten. */
        private void closeOpen() {
            FerraloomInventoryView view = open;
            if (view != null) {
                open = null;
                Bridge.server.pluginManager.callEvent(new InventoryCloseEvent(view));
            }
        }

        /** HumanEntity.closeInventory: the close event, then the server closes the menu. */
        void closeInventory() {
            closeOpen();
            InventoryNative.close(most(), least());
        }

        /** The server tells: the client closed {@code (kind, a, b)}. */
        void closed(int kind, long a, long b, String type) {
            FerraloomInventoryView view = view(kind, a, b, type);
            open = null;
            Bridge.server.pluginManager.callEvent(new InventoryCloseEvent(view));
        }

        /** The server tells: it is about to open {@code (kind, a, b)} for the player; true when cancelled. */
        boolean opening(int kind, long a, long b, String type) {
            FerraloomInventoryView view = new FerraloomInventoryView(player, top(kind, a, b, type));
            InventoryOpenEvent event = new InventoryOpenEvent(view);
            Bridge.server.pluginManager.callEvent(event);
            if (!event.isCancelled()) {
                open = view;
            }
            return event.isCancelled();
        }

        void update() {
            InventoryNative.resend(most(), least());
        }

        ItemStack cursor() {
            return Items.orAir(Items.read(InventoryNative.cursor(most(), least())));
        }

        void setCursor(ItemStack item) {
            InventoryNative.setCursor(most(), least(), Items.write(item));
        }
    }
}
