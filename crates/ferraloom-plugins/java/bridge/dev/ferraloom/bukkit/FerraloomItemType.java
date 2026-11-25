package dev.ferraloom.bukkit;

import com.google.common.base.Preconditions;
import com.google.common.collect.Multimap;
import java.util.function.Consumer;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.World;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeModifier;
import org.bukkit.block.BlockType;
import org.bukkit.inventory.CreativeCategory;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.ItemType;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.inventory.meta.EnchantmentStorageMeta;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.LeatherArmorMeta;
import org.bukkit.inventory.meta.PotionMeta;
import org.bukkit.inventory.meta.SkullMeta;

/**
 * CraftItemType: an item's prototype, as the server's item table has it (max stack size, max
 * damage, food, fuel...), read once on first ask ({@link InventoryNative#itemFacts}). The
 * {@code ItemType} registry's entries, so {@code Material.getMaxStackSize()} and
 * {@code ItemStack.getMaxStackSize()} answer the server's numbers.
 */
final class FerraloomItemType implements ItemType.Typed<ItemMeta> {
    private final NamespacedKey key;
    /** {max stack, max damage, edible, fuel, record, compostable}; null until asked. */
    private volatile int[] facts;

    FerraloomItemType(NamespacedKey key) {
        this.key = key;
    }

    private int[] facts() {
        int[] f = facts;
        if (f == null) {
            f = InventoryNative.itemFacts(key.toString());
            if (f == null) {
                // An item the server does not know: a plain one.
                f = new int[] {64, 0, 0, 0, 0, 0};
            }
            facts = f;
        }
        return f;
    }

    @Override
    public NamespacedKey getKey() {
        return key;
    }

    @Override
    public NamespacedKey getKeyOrThrow() {
        return key;
    }

    @Override
    public NamespacedKey getKeyOrNull() {
        return key;
    }

    @Override
    public boolean isRegistered() {
        return true;
    }

    @Override
    public String getTranslationKey() {
        return (hasBlockType() ? "block." : "item.") + key.getNamespace() + "." + key.getKey().replace('/', '.');
    }

    @Override
    public Typed<ItemMeta> typed() {
        return this;
    }

    @Override
    @SuppressWarnings("unchecked")
    public <M extends ItemMeta> Typed<M> typed(Class<M> type) {
        Preconditions.checkArgument(type.isAssignableFrom(getItemMetaClass()), "%s is not a %s", key, type.getSimpleName());
        return (Typed<M>) this;
    }

    @Override
    public ItemStack createItemStack() {
        return createItemStack(1);
    }

    @Override
    public ItemStack createItemStack(int amount) {
        return new ItemStack(asMaterial(), amount);
    }

    @Override
    public ItemStack createItemStack(Consumer<? super ItemMeta> meta) {
        return createItemStack(1, meta);
    }

    @Override
    public ItemStack createItemStack(int amount, Consumer<? super ItemMeta> meta) {
        ItemStack stack = createItemStack(amount);
        ItemMeta m = stack.getItemMeta();
        if (m != null && meta != null) {
            meta.accept(m);
            stack.setItemMeta(m);
        }
        return stack;
    }

    @Override
    public boolean hasBlockType() {
        return Registry.BLOCK.get(key) != null;
    }

    @Override
    public BlockType getBlockType() {
        BlockType block = Registry.BLOCK.get(key);
        Preconditions.checkArgument(block != null, "The item type %s has no corresponding block type", key);
        return block;
    }

    @Override
    public Class<ItemMeta> getItemMetaClass() {
        Material material = asMaterial();
        Class<? extends ItemMeta> type = switch (material == null ? "UNSPECIFIC" : FerraloomItemFactory.typeOf(material)) {
            case "SKULL" -> SkullMeta.class;
            case "POTION" -> PotionMeta.class;
            case "LEATHER_ARMOR" -> LeatherArmorMeta.class;
            case "BOOK", "BOOK_SIGNED" -> BookMeta.class;
            case "ENCHANTED" -> EnchantmentStorageMeta.class;
            default -> ItemMeta.class;
        };
        @SuppressWarnings("unchecked")
        Class<ItemMeta> erased = (Class<ItemMeta>) type;
        return erased;
    }

    @Override
    public int getMaxStackSize() {
        return facts()[0];
    }

    @Override
    public short getMaxDurability() {
        return (short) facts()[1];
    }

    @Override
    public boolean isEdible() {
        return facts()[2] != 0;
    }

    @Override
    public boolean isRecord() {
        return facts()[4] != 0;
    }

    @Override
    public boolean isFuel() {
        return facts()[3] != 0;
    }

    @Override
    public boolean isCompostable() {
        return facts()[5] != 0;
    }

    @Override
    public float getCompostChance() {
        Preconditions.checkArgument(isCompostable(), "The item type %s is not compostable", key);
        throw new UnsupportedOperationException("Ferraloom does not implement ItemType.getCompostChance() yet");
    }

    @Override
    public ItemType getCraftingRemainingItem() {
        String remainder = InventoryNative.itemRemainder(key.toString());
        NamespacedKey k = remainder == null ? null : NamespacedKey.fromString(remainder);
        return k == null ? null : Registry.ITEM.get(k);
    }

    @Override
    public Multimap<Attribute, AttributeModifier> getDefaultAttributeModifiers(EquipmentSlot slot) {
        throw new UnsupportedOperationException("Ferraloom does not implement ItemType.getDefaultAttributeModifiers() yet");
    }

    @SuppressWarnings({"deprecation", "removal"})
    @Override
    public CreativeCategory getCreativeCategory() {
        throw new UnsupportedOperationException("Ferraloom does not implement ItemType.getCreativeCategory() yet");
    }

    @Override
    public boolean isEnabledByFeature(World world) {
        return true;
    }

    @Override
    public Material asMaterial() {
        return Material.matchMaterial(key.toString());
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomItemType t && t.key.equals(key);
    }

    @Override
    public int hashCode() {
        return key.hashCode();
    }

    @Override
    public String toString() {
        return "FerraloomItemType{" + key + "}";
    }
}
