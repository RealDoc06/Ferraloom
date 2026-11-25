package dev.ferraloom.bukkit;

import com.google.common.base.Preconditions;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;

/**
 * CraftInventoryPlayer: 0-8 the hotbar, 9-35 the rest, 36-39 boots, leggings, chestplate and
 * helmet, 40 the off hand. The storage contents are 0-35, as addItem fills them.
 */
final class FerraloomPlayerInventory extends FerraloomInventory implements PlayerInventory {
    static final int SIZE = 41;
    private final FerraloomPlayer player;

    FerraloomPlayerInventory(FerraloomPlayer player) {
        super(0, player.id.getMostSignificantBits(), player.id.getLeastSignificantBits(), InventoryType.PLAYER, SIZE,
                player, null);
        this.player = player;
    }

    private ItemStack[] range(int from, int to) {
        ItemStack[] items = new ItemStack[to - from];
        for (int i = from; i < to; i++) {
            items[i - from] = getItem(i);
        }
        return items;
    }

    private void setRange(int from, int to, ItemStack[] items) {
        Preconditions.checkArgument(items == null || items.length <= to - from, "Invalid inventory size (%s); expected %s or less",
                items == null ? 0 : items.length, to - from);
        for (int i = from; i < to; i++) {
            setItem(i, items == null || i - from >= items.length ? null : items[i - from]);
        }
    }

    @Override
    public ItemStack[] getStorageContents() {
        return range(0, 36);
    }

    @Override
    public void setStorageContents(ItemStack[] items) {
        setRange(0, 36, items);
    }

    @Override
    public ItemStack[] getArmorContents() {
        return range(36, 40);
    }

    @Override
    public void setArmorContents(ItemStack[] items) {
        setRange(36, 40, items);
    }

    @Override
    public ItemStack[] getExtraContents() {
        return range(40, 41);
    }

    @Override
    public void setExtraContents(ItemStack[] items) {
        setRange(40, 41, items);
    }

    @Override
    public ItemStack getHelmet() {
        return getItem(39);
    }

    @Override
    public ItemStack getChestplate() {
        return getItem(38);
    }

    @Override
    public ItemStack getLeggings() {
        return getItem(37);
    }

    @Override
    public ItemStack getBoots() {
        return getItem(36);
    }

    @Override
    public void setHelmet(ItemStack item) {
        setItem(39, item);
    }

    @Override
    public void setChestplate(ItemStack item) {
        setItem(38, item);
    }

    @Override
    public void setLeggings(ItemStack item) {
        setItem(37, item);
    }

    @Override
    public void setBoots(ItemStack item) {
        setItem(36, item);
    }

    @Override
    public void setItem(EquipmentSlot slot, ItemStack item) {
        Preconditions.checkArgument(slot != null, "slot must not be null");
        switch (slot) {
            case HAND -> setItemInMainHand(item);
            case OFF_HAND -> setItemInOffHand(item);
            case FEET -> setBoots(item);
            case LEGS -> setLeggings(item);
            case CHEST -> setChestplate(item);
            case HEAD -> setHelmet(item);
            default -> throw new IllegalArgumentException("Not implemented. This is a bug");
        }
    }

    @Override
    public ItemStack getItem(EquipmentSlot slot) {
        Preconditions.checkArgument(slot != null, "slot must not be null");
        return switch (slot) {
            case HAND -> getItemInMainHand();
            case OFF_HAND -> getItemInOffHand();
            case FEET -> Items.orAir(getBoots());
            case LEGS -> Items.orAir(getLeggings());
            case CHEST -> Items.orAir(getChestplate());
            case HEAD -> Items.orAir(getHelmet());
            default -> throw new IllegalArgumentException("Not implemented. This is a bug");
        };
    }

    /** As CraftInventoryPlayer: never null, air for an empty hand. */
    @Override
    public ItemStack getItemInMainHand() {
        return Items.orAir(getItem(getHeldItemSlot()));
    }

    @Override
    public void setItemInMainHand(ItemStack item) {
        setItem(getHeldItemSlot(), item);
    }

    @Override
    public ItemStack getItemInOffHand() {
        return Items.orAir(getItem(40));
    }

    @Override
    public void setItemInOffHand(ItemStack item) {
        setItem(40, item);
    }

    @Override
    public ItemStack getItemInHand() {
        return getItemInMainHand();
    }

    @Override
    public void setItemInHand(ItemStack item) {
        setItemInMainHand(item);
    }

    @Override
    public int getHeldItemSlot() {
        return InventoryNative.heldSlot(player.id.getMostSignificantBits(), player.id.getLeastSignificantBits());
    }

    @Override
    public void setHeldItemSlot(int slot) {
        Preconditions.checkArgument(slot >= 0 && slot < 9, "Slot (%s) is not between 0 and %s inclusive", slot, 8);
        InventoryNative.setHeldSlot(player.id.getMostSignificantBits(), player.id.getLeastSignificantBits(), slot);
    }

    @Override
    public HumanEntity getHolder() {
        return player;
    }

    @Override
    public String toString() {
        return "FerraloomPlayerInventory{" + player.getName() + "}";
    }
}
