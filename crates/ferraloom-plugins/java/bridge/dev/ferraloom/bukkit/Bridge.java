package dev.ferraloom.bukkit;

import java.io.File;
import java.util.logging.Level;
import org.bukkit.Bukkit;
import org.bukkit.event.HandlerList;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;

/**
 * The server's lifecycle calls (crates/ferraloom-plugins/src/bridge.rs, same
 * names and signatures). Every call comes on the server thread, which is
 * Bukkit's main thread. Each is a wall: what a plugin throws through it is
 * logged here and answered with the default. The event entry points are per
 * area: {@link PlayerEvents}, {@link BlockEvents}.
 */
public final class Bridge {
    /**
     * Bit {@code i} of {@link #tick}'s answer: {@code EVENTS[i]} has
     * listeners. The bits are {@code Event}'s in src/events.rs; each area
     * keeps its own range (docs/ferraloom/plugins.md, "Fan-out").
     */
    private static final HandlerList[] EVENTS = new HandlerList[63];

    static {
        // Players: 0-15.
        EVENTS[0] = PlayerJoinEvent.getHandlerList();
        EVENTS[1] = PlayerQuitEvent.getHandlerList();
        EVENTS[2] = AsyncPlayerChatEvent.getHandlerList();
        // Blocks: 16-31.
        EVENTS[16] = BlockBreakEvent.getHandlerList();
        // Inventory: 48-55.
        EVENTS[48] = org.bukkit.event.inventory.InventoryClickEvent.getHandlerList();
        EVENTS[49] = org.bukkit.event.inventory.InventoryDragEvent.getHandlerList();
        EVENTS[50] = org.bukkit.event.inventory.InventoryOpenEvent.getHandlerList();
        EVENTS[51] = org.bukkit.event.inventory.InventoryCloseEvent.getHandlerList();
        EVENTS[52] = org.bukkit.event.inventory.InventoryMoveItemEvent.getHandlerList();
    }

    /** {@link #tick}'s bit for "the command labels changed: ask {@link #commands} again". */
    private static final long COMMANDS_CHANGED = 1L << 63;

    static FerraloomServer server;
    private static long sentCommands = Long.MIN_VALUE;

    private Bridge() {
    }

    /** CraftServer's constructor and loadPlugins: the server, then every plugin's onLoad. */
    public static void start(String directory, String version, String[] worlds, int[] environments, int maxPlayers,
            int port, String motd, boolean onlineMode) {
        try {
            server = new FerraloomServer(NativeLog.install(), new File(directory), version, worlds, environments,
                    maxPlayers, port, motd, onlineMode);
            Bukkit.setServer(server);
            server.loadPlugins();
        } catch (Throwable ex) {
            log(ex, "starting the plugins");
        }
    }

    /** Every plugin's onEnable, once the worlds are loaded; answers as {@link #tick} does. */
    public static long enable() {
        try {
            server.enablePlugins();
        } catch (Throwable ex) {
            log(ex, "enabling plugins");
        }
        return flags();
    }

    /** Every plugin's onDisable, in reverse load order, then the scheduler and the chat thread stop. */
    public static void disable() {
        try {
            server.disablePlugins();
            PlayerEvents.stop();
        } catch (Throwable ex) {
            log(ex, "disabling plugins");
        }
    }

    /** The scheduler's heartbeat for this tick; answers which events have listeners now. */
    public static long tick(long tick) {
        try {
            server.scheduler.heartbeat(tick);
        } catch (Throwable ex) {
            log(ex, "running scheduled tasks");
        }
        return flags();
    }

    private static long flags() {
        long flags = 0;
        for (int i = 0; i < EVENTS.length; i++) {
            if (EVENTS[i] != null && EVENTS[i].getRegisteredListeners().length > 0) {
                flags |= 1L << i;
            }
        }
        if (server.commandMap.version() != sentCommands) {
            flags |= COMMANDS_CHANGED;
        }
        return flags;
    }

    /** Every label plugins' commands answer to. */
    public static String[] commands() {
        sentCommands = server.commandMap.version();
        return server.commandMap.labels();
    }

    /** A console line: true when a plugin's command took it. */
    public static boolean consoleCommand(String line) {
        try {
            return server.dispatchPluginCommand(server.console, line);
        } catch (Throwable ex) {
            log(ex, "a console command");
            return true;
        }
    }

    static void log(Throwable ex, String what) {
        if (server != null) {
            server.logger.log(Level.SEVERE, "Ferraloom's plugin bridge failed " + what, ex);
        } else {
            ex.printStackTrace();
        }
    }
}
