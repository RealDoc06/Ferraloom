package dev.ferraloom.bukkit;

import java.util.function.Consumer;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.World;
import org.bukkit.block.BlockType;
import org.bukkit.block.data.BlockData;
import org.bukkit.inventory.ItemType;

/**
 * A {@link BlockType} (CraftBlockType): {@code Registry.BLOCK}'s entries ({@link FerraloomRegistries}), and
 * so what {@code Material.isAir()}, {@code isSolid()}, {@code hasGravity()} and friends answer. Its facts are
 * the block's default state's, from the server's block table ({@link Blocks}).
 */
final class FerraloomBlockType implements BlockType.Typed<BlockData> {
    private final NamespacedKey key;
    private Blocks.Kind kind;

    FerraloomBlockType(NamespacedKey key) {
        this.key = key;
    }

    /** The server's block of this key; IllegalStateException for one the server does not have. */
    Blocks.Kind kind() {
        if (kind == null) {
            kind = Blocks.kind(key.toString());
            if (kind == null) {
                throw new IllegalStateException("The server has no block " + key);
            }
        }
        return kind;
    }

    @Override
    public Typed<BlockData> typed() {
        return this;
    }

    @Override
    @SuppressWarnings("unchecked")
    public <B extends BlockData> Typed<B> typed(Class<B> type) {
        if (!type.isAssignableFrom(getBlockDataClass())) {
            throw new IllegalArgumentException("Cannot type block type " + key + " to blockdata type " + type.getSimpleName());
        }
        return (Typed<B>) (Typed<?>) this;
    }

    @Override
    public boolean hasItemType() {
        // CraftBlockType.hasItemType: air is its own item.
        return key.getKey().equals("air") || !"minecraft:air".equals(kind().item);
    }

    @Override
    public ItemType getItemType() {
        if (!hasItemType()) {
            throw new IllegalStateException("The block type " + key + " has no corresponding item type");
        }
        return Registry.ITEM.get(NamespacedKey.fromString(kind().item));
    }

    @Override
    public Class<BlockData> getBlockDataClass() {
        @SuppressWarnings("unchecked")
        Class<BlockData> c = (Class<BlockData>) kind().dataClass();
        return c;
    }

    @Override
    public BlockData createBlockData() {
        return Blocks.data(kind().defaultState);
    }

    @Override
    public BlockData createBlockData(Consumer<? super BlockData> consumer) {
        BlockData data = createBlockData();
        if (consumer != null) {
            consumer.accept(data);
        }
        return data;
    }

    @Override
    public BlockData createBlockData(String data) {
        return Blocks.parse(asMaterial(), data);
    }

    @Override
    public boolean isSolid() {
        return kind().is(Blocks.SOLID);
    }

    @Override
    public boolean isBurnable() {
        return kind().is(Blocks.BURNABLE);
    }

    @Override
    public boolean isOccluding() {
        return kind().is(Blocks.OCCLUDING);
    }

    @Override
    public boolean hasGravity() {
        return kind().is(Blocks.GRAVITY);
    }

    @Override
    public float getHardness() {
        return kind().hardness;
    }

    @Override
    public float getBlastResistance() {
        return kind().resistance;
    }

    @Override
    public float getSlipperiness() {
        return kind().friction;
    }

    @Override
    public boolean isAir() {
        return kind().is(Blocks.AIR);
    }

    /** Every block of 26.3 without experimental features is enabled. */
    @Override
    public boolean isEnabledByFeature(World world) {
        return true;
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
        return "block." + key.getNamespace() + "." + key.getKey();
    }

    @Override
    public Material asMaterial() {
        return Material.matchMaterial(key.toString());
    }

    @Override
    public boolean isFlammable() {
        throw new UnsupportedOperationException("Ferraloom does not implement BlockType.isFlammable() yet");
    }

    @Override
    public boolean isInteractable() {
        throw new UnsupportedOperationException("Ferraloom does not implement BlockType.isInteractable() yet");
    }

    @Override
    public String toString() {
        return "CraftBlockType{key=" + key + "}";
    }
}
