package dev.ferraloom.bukkit;

import com.google.common.base.Preconditions;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.event.inventory.InventoryType.SlotType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/**
 * CraftInventoryView: a menu a player has open, its top inventory over the player's own. Raw
 * slots count the top's first, then the player's: for the player's own inventory menu (type
 * CRAFTING) 0 the result, 1-4 the grid, 5-8 the armour, 9-35 the rest, 36-44 the hotbar and 45
 * the off hand; for a container its slots, then 27 inventory slots and the hotbar.
 */
final class FerraloomInventoryView implements InventoryView {
    final FerraloomPlayer player;
    final FerraloomInventory top;
    private final String original;
    private String title;

    FerraloomInventoryView(FerraloomPlayer player, FerraloomInventory top) {
        this.player = player;
        this.top = top;
        this.original = top.title;
        this.title = top.title;
    }

    @Override
    public Inventory getTopInventory() {
        return top;
    }

    @Override
    public Inventory getBottomInventory() {
        return player.getInventory();
    }

    @Override
    public HumanEntity getPlayer() {
        return player;
    }

    @Override
    public InventoryType getType() {
        return top.getType();
    }

    private boolean playerMenu() {
        return getType() == InventoryType.CRAFTING || getType() == InventoryType.CREATIVE;
    }

    @Override
    public void setItem(int slot, ItemStack item) {
        Inventory inventory = getInventory(slot);
        if (inventory != null) {
            inventory.setItem(convertSlot(slot), item);
        }
    }

    /** CraftInventoryView.getItem: the slot's item, air when it is empty; null outside. */
    @Override
    public ItemStack getItem(int slot) {
        Inventory inventory = getInventory(slot);
        return inventory == null ? null : Items.orAir(inventory.getItem(convertSlot(slot)));
    }

    @Override
    public void setCursor(ItemStack item) {
        player.setItemOnCursor(item);
    }

    @Override
    public ItemStack getCursor() {
        return player.getItemOnCursor();
    }

    @Override
    public Inventory getInventory(int rawSlot) {
        if (rawSlot == OUTSIDE || rawSlot == -1) {
            return null;
        }
        Preconditions.checkArgument(rawSlot >= 0, "Negative, non outside slot %s", rawSlot);
        Preconditions.checkArgument(rawSlot < countSlots(), "Slot %s greater than inventory slot count", rawSlot);
        return rawSlot < top.getSize() ? top : getBottomInventory();
    }

    /** InventoryView.convertSlot: a raw slot as its inventory's index. */
    @Override
    public int convertSlot(int rawSlot) {
        int numInTop = top.getSize();
        if (rawSlot < numInTop) {
            return rawSlot;
        }
        int slot = rawSlot - numInTop;
        if (playerMenu()) {
            if (slot < 4) {
                return 39 - slot;
            } else if (slot > 39) {
                return slot;
            }
            slot -= 4;
        }
        return slot >= 27 ? slot - 27 : slot + 9;
    }

    /** CraftInventoryView.getSlotType. */
    @Override
    public SlotType getSlotType(int slot) {
        SlotType type = SlotType.CONTAINER;
        if (slot >= 0 && slot < top.getSize()) {
            switch (getType()) {
                case BLAST_FURNACE, FURNACE, SMOKER -> type = slot == 2 ? SlotType.RESULT : slot == 1 ? SlotType.FUEL : SlotType.CRAFTING;
                case BREWING -> type = slot == 3 ? SlotType.FUEL : SlotType.CRAFTING;
                case ENCHANTING -> type = SlotType.CRAFTING;
                case WORKBENCH, CRAFTING -> type = slot == 0 ? SlotType.RESULT : SlotType.CRAFTING;
                case BEACON, ANVIL, GRINDSTONE, CARTOGRAPHY, LOOM, STONECUTTER, SMITHING, MERCHANT, CRAFTER ->
                    type = slot == top.getSize() - 1 && getType() != InventoryType.BEACON && getType() != InventoryType.CRAFTER
                        ? SlotType.RESULT : SlotType.CRAFTING;
                default -> {
                }
            }
        } else if (slot < 0) {
            type = SlotType.OUTSIDE;
        } else if (getType() == InventoryType.CRAFTING) {
            if (slot < 9) {
                type = SlotType.ARMOR;
            } else if (slot > 35) {
                type = SlotType.QUICKBAR;
            }
        } else if (slot >= countSlots() - 9) {
            // countSlots() here ends with the hotbar: no armour or off hand after it.
            type = SlotType.QUICKBAR;
        }
        return type;
    }

    @Override
    public void close() {
        player.closeInventory();
    }

    /** The menu's slots: the top's and the player's 36 (with armour and off hand, 46, for the player's own). */
    @Override
    public int countSlots() {
        return playerMenu() ? 46 : top.getSize() + 36;
    }

    @SuppressWarnings("removal")
    @Override
    public boolean setProperty(Property prop, int value) {
        return false;
    }

    @Override
    public String getTitle() {
        return title;
    }

    @Override
    public String getOriginalTitle() {
        return original;
    }

    /** CraftInventoryView.setTitle: the menu opens again under the new title. */
    @Override
    public void setTitle(String title) {
        this.title = title;
        if (player.inventories.open() == this) {
            player.inventories.reopen(this);
        }
    }

    @Override
    public String toString() {
        return "FerraloomInventoryView{" + getType() + ", " + title + "}";
    }
}
