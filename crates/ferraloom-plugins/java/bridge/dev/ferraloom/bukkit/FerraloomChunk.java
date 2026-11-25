package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubChunk;
import java.util.Collection;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.plugin.Plugin;

/** A chunk position in a world, as CraftChunk is: whether it is loaded or generated is asked when called. */
final class FerraloomChunk extends StubChunk {
    final FerraloomWorld world;
    final int x;
    final int z;

    FerraloomChunk(FerraloomWorld world, int x, int z) {
        this.world = world;
        this.x = x;
        this.z = z;
    }

    @Override
    public int getX() {
        return x;
    }

    @Override
    public int getZ() {
        return z;
    }

    @Override
    public World getWorld() {
        return world;
    }

    /** A block of this chunk, by its coordinates within it (x and z 0 to 15). */
    @Override
    public Block getBlock(int bx, int by, int bz) {
        return new FerraloomBlock(world, x << 4 | bx & 15, by, z << 4 | bz & 15);
    }

    @Override
    public boolean isLoaded() {
        return world.isChunkLoaded(x, z);
    }

    @Override
    public boolean isGenerated() {
        return world.isChunkGenerated(x, z);
    }

    @Override
    public boolean load(boolean generate) {
        return world.loadChunk(x, z, generate);
    }

    @Override
    public boolean load() {
        return world.loadChunk(x, z, true);
    }

    @Override
    public boolean unload(boolean save) {
        return world.unloadChunk(x, z, save);
    }

    @Override
    public boolean unload() {
        return world.unloadChunk(x, z);
    }

    @Override
    public boolean addPluginChunkTicket(Plugin plugin) {
        return world.addPluginChunkTicket(x, z, plugin);
    }

    @Override
    public boolean removePluginChunkTicket(Plugin plugin) {
        return world.removePluginChunkTicket(x, z, plugin);
    }

    @Override
    public Collection<Plugin> getPluginChunkTickets() {
        return world.getPluginChunkTickets(x, z);
    }

    /** CraftChunk.isSlimeChunk: the seeded random of the chunk, one in ten. */
    @Override
    public boolean isSlimeChunk() {
        long seed = world.getSeed() + (long) (x * x * 4987142) + (long) (x * 5947611) + (long) (z * z) * 4392871L
                + (long) (z * 389711) ^ 987234911L;
        return new java.util.Random(seed).nextInt(10) == 0;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomChunk c && c.x == x && c.z == z && c.world.equals(world);
    }

    @Override
    public int hashCode() {
        return 31 * (31 * world.hashCode() + x) + z;
    }

    @Override
    public String toString() {
        return "CraftChunk{x=" + x + "z=" + z + "}";
    }
}
