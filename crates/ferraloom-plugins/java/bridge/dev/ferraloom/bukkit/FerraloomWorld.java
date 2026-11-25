package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubWorld;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.block.Block;
import org.bukkit.entity.Player;

/** One of the server's dimensions, named as CraftBukkit names them: {@code world}, {@code world_nether}, {@code world_the_end}. */
final class FerraloomWorld extends StubWorld {
    final FerraloomServer server;
    /** Its place in the list the server gave {@link Bridge#start}: how {@link WorldNative} names it. */
    final int index;
    final String name;
    final Environment environment;
    final UUID uid;

    FerraloomWorld(FerraloomServer server, int index, String name, Environment environment) {
        this.server = server;
        this.index = index;
        this.name = name;
        this.environment = environment;
        // Stable across restarts until uid.dat is read (CraftWorld reads it from the level folder).
        this.uid = UUID.nameUUIDFromBytes(("Ferraloom world " + name).getBytes(StandardCharsets.UTF_8));
    }

    @Override
    public String getName() {
        return name;
    }

    @Override
    public UUID getUID() {
        return uid;
    }

    @Override
    public Environment getEnvironment() {
        return environment;
    }

    @Override
    public NamespacedKey getKey() {
        return NamespacedKey.minecraft(switch (environment) {
            case NETHER -> "the_nether";
            case THE_END -> "the_end";
            default -> "overworld";
        });
    }

    /** DimensionType's min_y: the vanilla dimension types. */
    @Override
    public int getMinHeight() {
        return environment == Environment.NORMAL ? -64 : 0;
    }

    /** DimensionType's min_y + height. */
    @Override
    public int getMaxHeight() {
        return environment == Environment.NORMAL ? 320 : 256;
    }

    @Override
    public long getFullTime() {
        return WorldNative.worldTime(index, true);
    }

    @Override
    public long getTime() {
        return WorldNative.worldTime(index, false);
    }

    @Override
    public List<Player> getPlayers() {
        List<Player> here = new ArrayList<>();
        for (Player p : server.getOnlinePlayers()) {
            if (equals(p.getWorld())) {
                here.add(p);
            }
        }
        return here;
    }

    @Override
    public Block getBlockAt(int x, int y, int z) {
        return new FerraloomBlock(this, x, y, z, null);
    }

    @Override
    public Block getBlockAt(Location location) {
        return getBlockAt(location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomWorld w && w.index == index && w.server == server;
    }

    @Override
    public int hashCode() {
        return uid.hashCode();
    }

    @Override
    public String toString() {
        return "FerraloomWorld{name=" + name + "}";
    }
}
