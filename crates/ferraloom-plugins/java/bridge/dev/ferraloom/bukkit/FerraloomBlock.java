package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubBlock;
import java.util.Objects;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;

/**
 * A block position in a world, as CraftBlock is: its type is read from the
 * world when asked (or the state an event saw before it changed).
 */
final class FerraloomBlock extends StubBlock {
    final FerraloomWorld world;
    final int x;
    final int y;
    final int z;
    /** The state's name an event captured, or null to read the world. */
    private final String state;

    FerraloomBlock(FerraloomWorld world, int x, int y, int z, String state) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
        this.state = state;
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

    /** The state's block as a Material; air where no chunk is loaded (as CraftBlock reads an empty section). */
    @Override
    public Material getType() {
        String name = state != null ? state : WorldNative.blockAt(world.index, x, y, z);
        if (name == null) {
            return Material.AIR;
        }
        int props = name.indexOf('[');
        Material type = Material.matchMaterial(props < 0 ? name : name.substring(0, props));
        return type == null ? Material.AIR : type;
    }

    /** Material.isAir asks the block registry, which Ferraloom does not provide yet: the three airs by name. */
    @Override
    public boolean isEmpty() {
        Material type = getType();
        return type == Material.AIR || type == Material.CAVE_AIR || type == Material.VOID_AIR;
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
        return "FerraloomBlock{pos=" + x + "," + y + "," + z + ",type=" + getType() + ",world=" + world.name + "}";
    }
}
