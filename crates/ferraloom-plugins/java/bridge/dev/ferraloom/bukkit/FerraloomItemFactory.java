package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubItemFactory;
import java.util.Map;
import org.bukkit.Color;
import org.bukkit.Material;
import org.bukkit.entity.EntityType;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/**
 * CraftItemFactory: the meta class a material takes, and comparing and converting metas. An item
 * with no meta and one whose meta patches nothing are the same item.
 */
final class FerraloomItemFactory extends StubItemFactory {
    static final FerraloomItemFactory INSTANCE = new FerraloomItemFactory();

    static {
        FerraloomItemMeta.Serialized.register();
    }

    private FerraloomItemFactory() {
    }

    /** CraftItemFactory.getItemMeta's choice of class, over {@code patch}. */
    static FerraloomItemMeta metaFor(Material material, Map<String, Object> patch) {
        return metaOfType(typeOf(material), patch);
    }

    /** CraftMetaItem's {@code meta-type} for a material. */
    static String typeOf(Material material) {
        String name = material.name();
        return switch (name) {
            case "POTION", "SPLASH_POTION", "LINGERING_POTION", "TIPPED_ARROW" -> "POTION";
            case "LEATHER_HELMET", "LEATHER_CHESTPLATE", "LEATHER_LEGGINGS", "LEATHER_BOOTS", "LEATHER_HORSE_ARMOR",
                    "WOLF_ARMOR" -> "LEATHER_ARMOR";
            case "WRITABLE_BOOK" -> "BOOK";
            case "WRITTEN_BOOK" -> "BOOK_SIGNED";
            case "ENCHANTED_BOOK" -> "ENCHANTED";
            default -> (name.endsWith("_HEAD") || name.endsWith("_SKULL")) && !name.contains("WALL") && !name.equals("PISTON_HEAD")
                    ? "SKULL" : "UNSPECIFIC";
        };
    }

    static FerraloomItemMeta metaOfType(String type, Map<String, Object> patch) {
        return switch (type) {
            case "SKULL" -> new FerraloomMetas.Skull(patch);
            case "POTION" -> new FerraloomMetas.Potion(patch);
            case "LEATHER_ARMOR" -> new FerraloomMetas.Leather(patch);
            case "BOOK" -> new FerraloomMetas.Book(patch, false);
            case "BOOK_SIGNED" -> new FerraloomMetas.Book(patch, true);
            case "ENCHANTED" -> new FerraloomMetas.EnchantedBook(patch);
            default -> new FerraloomItemMeta(patch);
        };
    }

    private static boolean fits(ItemMeta meta, Material material) {
        if (material == null || Items.isAir(material)) {
            return false;
        }
        return meta instanceof FerraloomItemMeta m
                && (m.getClass() == FerraloomItemMeta.class || m.metaType().equals(typeOf(material)));
    }

    private static FerraloomItemMeta check(ItemMeta meta) {
        if (meta != null && !(meta instanceof FerraloomItemMeta)) {
            throw new IllegalArgumentException("Meta of " + meta.getClass().getName() + " not created by " + FerraloomItemFactory.class.getName());
        }
        return (FerraloomItemMeta) meta;
    }

    @Override
    public ItemMeta getItemMeta(Material material) {
        return material == null || Items.isAir(material) ? null : metaFor(material, Nbt.map());
    }

    @Override
    public boolean isApplicable(ItemMeta meta, ItemStack stack) {
        return stack != null && isApplicable(meta, stack.getType());
    }

    @Override
    public boolean isApplicable(ItemMeta meta, Material material) {
        check(meta);
        return meta == null || fits(meta, material);
    }

    @Override
    public boolean equals(ItemMeta a, ItemMeta b) {
        FerraloomItemMeta x = check(a);
        FerraloomItemMeta y = check(b);
        if (x == y) {
            return true;
        }
        if (x == null || y == null) {
            return (x == null ? y : x).isEmpty();
        }
        return Nbt.same(x.patch(), y.patch());
    }

    @Override
    public ItemMeta asMetaFor(ItemMeta meta, ItemStack stack) {
        return asMetaFor(meta, stack.getType());
    }

    @Override
    public ItemMeta asMetaFor(ItemMeta meta, Material material) {
        FerraloomItemMeta m = check(meta);
        if (m == null || material == null || Items.isAir(material)) {
            return null;
        }
        return m.metaType().equals(typeOf(material)) ? m : metaFor(material, Nbt.copy(m.patch()));
    }

    @Override
    public Color getDefaultLeatherColor() {
        return FerraloomMetas.Leather.DEFAULT;
    }

    @Override
    public Material getSpawnEgg(EntityType type) {
        return type == null || type.getKey() == null ? null : Material.matchMaterial(type.getKey().getKey() + "_spawn_egg");
    }
}
