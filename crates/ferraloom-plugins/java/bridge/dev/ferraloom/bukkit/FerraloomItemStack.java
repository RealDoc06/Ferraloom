package dev.ferraloom.bukkit;

import java.util.Map;
import org.bukkit.Material;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/**
 * CraftItemStack's mirror: an item a plugin got from an inventory slot, whose changes go to that
 * slot, as {@code player.getInventory().getItemInMainHand().setAmount(n - 1)} expects. Once the
 * slot holds another stack (the player moved it, a plugin replaced it) the mirror is a plain
 * item, as CraftBukkit's is once its handle left the slot. A copy ({@link #clone}) is always plain.
 */
final class FerraloomItemStack extends ItemStack {
    private final FerraloomInventory inventory;
    private final int slot;
    /** What the slot held when this mirror last read or wrote it; null once detached. */
    private byte[] last;

    FerraloomItemStack(FerraloomInventory inventory, int slot, byte[] bytes, ItemStack read) {
        super();
        super.setType(read.getType());
        super.setAmount(read.getAmount());
        if (read.hasItemMeta()) {
            super.setItemMeta(read.getItemMeta());
        }
        this.inventory = inventory;
        this.slot = slot;
        this.last = bytes;
    }

    /** The change, written to the slot while it still holds what this mirror saw. */
    private void flush() {
        if (last == null) {
            return;
        }
        byte[] now = Items.write(this);
        if (InventoryNative.replaceSlot(inventory.kind, inventory.a, inventory.b, slot, last, now)) {
            last = now;
        } else {
            last = null;
        }
    }

    @Override
    public void setType(Material type) {
        super.setType(type);
        flush();
    }

    @Override
    public void setAmount(int amount) {
        super.setAmount(amount);
        flush();
    }

    @Override
    public boolean setItemMeta(ItemMeta meta) {
        boolean set = super.setItemMeta(meta);
        flush();
        return set;
    }

    @Override
    public void addUnsafeEnchantment(Enchantment enchantment, int level) {
        super.addUnsafeEnchantment(enchantment, level);
        flush();
    }

    @Override
    public void addUnsafeEnchantments(Map<Enchantment, Integer> enchantments) {
        super.addUnsafeEnchantments(enchantments);
        flush();
    }

    @Override
    public int removeEnchantment(Enchantment enchantment) {
        int level = super.removeEnchantment(enchantment);
        flush();
        return level;
    }

    @Override
    public void removeEnchantments() {
        super.removeEnchantments();
        flush();
    }

    @Override
    public ItemStack clone() {
        return new ItemStack(this);
    }
}
