package dev.ferraloom.bukkit;

import java.util.UUID;
import org.bukkit.event.block.BlockBreakEvent;

/** The blocks area's event entry points (crates/ferraloom-plugins/src/worlds.rs), on the server thread. */
public final class BlockEvents {
    private BlockEvents() {
    }

    /** ServerPlayerGameMode.destroyBlock's BlockBreakEvent, before anything breaks: true when cancelled. */
    public static boolean blockBreak(long most, long least, int world, int x, int y, int z, String state) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        FerraloomWorld in = Bridge.server.world(world);
        if (player == null || in == null) {
            return false;
        }
        try {
            BlockBreakEvent event = new BlockBreakEvent(new FerraloomBlock(in, x, y, z, state), player);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled();
        } catch (Throwable ex) {
            Bridge.log(ex, "a block breaking");
            return false;
        }
    }
}
