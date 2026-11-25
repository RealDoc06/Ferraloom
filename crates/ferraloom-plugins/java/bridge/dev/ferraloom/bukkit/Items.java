package dev.ferraloom.bukkit;

import java.util.Map;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/**
 * Item stacks to and from the server: the entry points every area converts items with.
 * {@link #read} takes what src/inventory.rs's {@code to_java} gives (an {@code ItemStack.CODEC}
 * compound in network NBT, null for an empty slot) and {@link #write} gives back what its
 * {@code from_java} takes. Read items only when a plugin asks for them: each read allocates the
 * stack, its meta and its components.
 */
public final class Items {
    private Items() {
    }

    /** A stack from the server's bytes; null for none. */
    public static ItemStack read(byte[] bytes) {
        return bytes == null ? null : fromTag(Nbt.compound(Nbt.read(bytes)));
    }

    /** A stack's bytes for the server; null for null, air or an amount below one. */
    public static byte[] write(ItemStack stack) {
        Map<String, Object> tag = toTag(stack);
        return tag == null ? null : Nbt.write(tag);
    }

    /** A stack as never empty: air for none, as CraftItemStack.asBukkitCopy answers an empty slot. */
    public static ItemStack orAir(ItemStack stack) {
        return stack == null ? new ItemStack(Material.AIR) : stack;
    }

    /**
     * Air of any kind: no item. Not Material.isAir, which asks the server's block table: items are
     * converted on any thread (an async task's ItemStack), and this check needs no server.
     */
    public static boolean isAir(Material material) {
        return material == Material.AIR || material == Material.CAVE_AIR || material == Material.VOID_AIR;
    }

    /** The material of an item id ({@code minecraft:stone}); null for one the API does not name. */
    public static Material material(String id) {
        return id == null ? null : Material.matchMaterial(id);
    }

    /** ItemStack.CODEC's compound as a stack; null for none or an item the API does not name. */
    static ItemStack fromTag(Map<String, Object> tag) {
        if (tag == null || !(tag.get("id") instanceof String id)) {
            return null;
        }
        Material material = material(id);
        int count = Nbt.intOr(tag.get("count"), 1);
        if (material == null || Items.isAir(material) || count <= 0) {
            return null;
        }
        ItemStack stack = new ItemStack(material, count);
        Map<String, Object> patch = Nbt.compound(tag.get("components"));
        if (patch != null && !patch.isEmpty()) {
            stack.setItemMeta(FerraloomItemFactory.metaFor(material, patch));
        }
        return stack;
    }

    /** A stack as ItemStack.CODEC's compound; null for none. */
    static Map<String, Object> toTag(ItemStack stack) {
        if (stack == null || Items.isAir(stack.getType()) || stack.getAmount() <= 0) {
            return null;
        }
        Map<String, Object> tag = Nbt.map();
        tag.put("id", stack.getType().getKey().toString());
        tag.put("count", stack.getAmount());
        if (stack.hasItemMeta()) {
            ItemMeta meta = stack.getItemMeta();
            if (meta instanceof FerraloomItemMeta m && !m.isEmpty()) {
                tag.put("components", m.patch());
            }
        }
        return tag;
    }
}
