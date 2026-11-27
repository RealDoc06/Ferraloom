package dev.ferraloom.bukkit;

import org.bukkit.inventory.ItemStack;

/** Item's own methods, for item entities ({@link Entities} routes them here). */
final class FerraloomItem {
    final FerraloomEntity core;

    FerraloomItem(FerraloomEntity core) {
        this.core = core;
    }

    /** A copy, as CraftItem's is of the entity's stack; air once the item is gone. */
    public ItemStack getItemStack() {
        return Items.orAir(Items.read(EntityNative.itemStack(core.id, core.generation)));
    }

    public void setItemStack(ItemStack stack) {
        EntityNative.setItemStack(core.id, core.generation, Items.write(stack));
    }
}
