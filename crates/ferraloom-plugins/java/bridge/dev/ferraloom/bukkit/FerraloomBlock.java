package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubBlock;
import java.util.Objects;
import org.bukkit.Chunk;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.World;
import org.bukkit.block.Biome;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.BlockData;

/**
 * A block position in a world, as CraftBlock is: its state is read from the world when asked (one native
 * call answering a state id), or is the state an event saw before it changed. Reads go through the level
 * the server thread owns (a parked dimension is read in place, never a generation worker's copy); where no
 * chunk is loaded the block reads as air.
 */
final class FerraloomBlock extends StubBlock {
    final FerraloomWorld world;
    final int x;
    final int y;
    final int z;
    /** The state an event captured, or -1 to read the world. */
    private final int captured;

    FerraloomBlock(FerraloomWorld world, int x, int y, int z, int captured) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
        this.captured = captured;
    }

    FerraloomBlock(FerraloomWorld world, int x, int y, int z) {
        this(world, x, y, z, -1);
    }

    /** The state here: the captured one, else the world's (air where no chunk is loaded). */
    int state() {
        if (captured >= 0) {
            return captured;
        }
        int state = BlockNative.state(world.index, x, y, z);
        return state >= 0 ? state : Blocks.kind("minecraft:air").defaultState;
    }

    @Override
    public int getX() {
        return x;
    }

    @Override
    public int getY() {
        return y;
    }

    @Override
    public int getZ() {
        return z;
    }

    @Override
    public World getWorld() {
        return world;
    }

    @Override
    public Location getLocation() {
        return new Location(world, x, y, z);
    }

    @Override
    public Location getLocation(Location into) {
        if (into != null) {
            into.setWorld(world);
            into.setX(x);
            into.setY(y);
            into.setZ(z);
            into.setYaw(0);
            into.setPitch(0);
        }
        return into;
    }

    @Override
    public Chunk getChunk() {
        return world.getChunkAt(x >> 4, z >> 4);
    }

    @Override
    public Material getType() {
        return Blocks.kind(state()).material();
    }

    @Override
    public BlockData getBlockData() {
        return Blocks.data(state());
    }

    @Override
    public Block getRelative(int dx, int dy, int dz) {
        return new FerraloomBlock(world, x + dx, y + dy, z + dz);
    }

    @Override
    public Block getRelative(BlockFace face) {
        return getRelative(face, 1);
    }

    @Override
    public Block getRelative(BlockFace face, int distance) {
        return getRelative(face.getModX() * distance, face.getModY() * distance, face.getModZ() * distance);
    }

    /** CraftBlock.getFace: the face whose step reaches {@code block}, null for none. */
    @Override
    public BlockFace getFace(Block block) {
        for (BlockFace face : BlockFace.values()) {
            if (x + face.getModX() == block.getX() && y + face.getModY() == block.getY()
                    && z + face.getModZ() == block.getZ()) {
                return face;
            }
        }
        return null;
    }

    @Override
    public void setBlockData(BlockData data) {
        setBlockData(data, true);
    }

    @Override
    public void setBlockData(BlockData data, boolean applyPhysics) {
        if (data == null) {
            throw new IllegalArgumentException("BlockData cannot be null");
        }
        BlockNative.setState(world.index, x, y, z, Blocks.state(data), applyPhysics);
    }

    @Override
    public void setType(Material type) {
        setType(type, true);
    }

    @Override
    public void setType(Material type, boolean applyPhysics) {
        Blocks.Kind kind = type == null ? null : Blocks.kind(type);
        if (kind == null) {
            throw new IllegalArgumentException("Material must be a block: " + type);
        }
        BlockNative.setState(world.index, x, y, z, kind.defaultState, applyPhysics);
    }

    @Override
    public BlockState getState() {
        return FerraloomBlockState.of(world, x, y, z, state());
    }

    @Override
    public byte getLightLevel() {
        return (byte) (light() & 15);
    }

    @Override
    public byte getLightFromSky() {
        return (byte) (light() >> 8 & 15);
    }

    @Override
    public byte getLightFromBlocks() {
        return (byte) (light() >> 4 & 15);
    }

    /** {@code sky << 8 | block << 4 | max}; an unloaded chunk reads as daylight with no block light. */
    private int light() {
        int light = BlockNative.light(world.index, x, y, z);
        return light >= 0 ? light : 15 << 8 | 15;
    }

    @Override
    public Biome getBiome() {
        String key = BlockNative.biome(world.index, x, y, z);
        Biome biome = key == null ? null : Registry.BIOME.get(NamespacedKey.fromString(key));
        return biome != null ? biome : Biome.PLAINS;
    }

    /** Material.isAir asks the block registry: air, cave air and void air. */
    @Override
    public boolean isEmpty() {
        return Blocks.kind(state()).is(Blocks.AIR);
    }

    /** CraftBlock.isLiquid: water, lava and bubble columns. */
    @Override
    public boolean isLiquid() {
        String name = Blocks.kind(state()).name;
        return name.equals("minecraft:water") || name.equals("minecraft:lava") || name.equals("minecraft:bubble_column");
    }

    @Override
    public boolean breakNaturally() {
        return BlockNative.breakNaturally(world.index, x, y, z);
    }

    @Override
    public String getTranslationKey() {
        NamespacedKey key = Blocks.kind(state()).key;
        return "block." + key.getNamespace() + "." + key.getKey();
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomBlock b && b.x == x && b.y == y && b.z == z && b.world.equals(world);
    }

    @Override
    public int hashCode() {
        return Objects.hash(world, x, y, z);
    }

    @Override
    public String toString() {
        return "CraftBlock{pos=" + x + "," + y + "," + z + ",type=" + getType() + ",data=" + getBlockData()
                + ",world=" + world.name + "}";
    }
}
