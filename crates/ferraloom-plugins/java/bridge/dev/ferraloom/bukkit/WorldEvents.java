package dev.ferraloom.bukkit;

import org.bukkit.event.weather.ThunderChangeEvent;
import org.bukkit.event.weather.WeatherChangeEvent;
import org.bukkit.event.world.ChunkLoadEvent;
import org.bukkit.event.world.ChunkUnloadEvent;
import org.bukkit.event.world.WorldInitEvent;
import org.bukkit.event.world.WorldLoadEvent;
import org.bukkit.event.world.WorldSaveEvent;

/**
 * The worlds area's event entry points (crates/ferraloom-plugins/src/worlds.rs, same names and signatures),
 * on the server thread, each called only while its event has a listener; and the world load events, fired
 * here at start.
 */
public final class WorldEvents {
    private WorldEvents() {
    }

    /** ChunkLoadEvent ({@code loaded}) or ChunkUnloadEvent, after the chunk came or before it goes. */
    public static void chunk(int world, int x, int z, boolean loaded, boolean newChunk) {
        FerraloomWorld in = Bridge.server.world(world);
        if (in == null) {
            return;
        }
        try {
            FerraloomChunk chunk = new FerraloomChunk(in, x, z);
            Bridge.server.pluginManager.callEvent(loaded ? new ChunkLoadEvent(chunk, newChunk) : new ChunkUnloadEvent(chunk));
        } catch (Throwable ex) {
            Bridge.log(ex, "a chunk event");
        }
    }

    /** WeatherChangeEvent, or ThunderChangeEvent ({@code thunder}), before the change: true when cancelled. */
    public static boolean weather(int world, boolean thunder, boolean to) {
        FerraloomWorld in = Bridge.server.world(world);
        if (in == null) {
            return false;
        }
        try {
            if (thunder) {
                ThunderChangeEvent event = new ThunderChangeEvent(in, to);
                Bridge.server.pluginManager.callEvent(event);
                return event.isCancelled();
            }
            WeatherChangeEvent event = new WeatherChangeEvent(in, to);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled();
        } catch (Throwable ex) {
            Bridge.log(ex, "a weather change");
            return false;
        }
    }

    /** ServerLevel.save's WorldSaveEvent. */
    public static void save(int world) {
        FerraloomWorld in = Bridge.server.world(world);
        if (in == null) {
            return;
        }
        try {
            Bridge.server.pluginManager.callEvent(new WorldSaveEvent(in));
        } catch (Throwable ex) {
            Bridge.log(ex, "a world save");
        }
    }

    /**
     * CraftServer's world loading: WorldInitEvent, then WorldLoadEvent, for each world, once the STARTUP
     * plugins are enabled and before the POSTWORLD ones (FerraloomServer.enablePlugins).
     */
    static void loaded(FerraloomServer server) {
        for (FerraloomWorld w : server.worlds) {
            try {
                server.pluginManager.callEvent(new WorldInitEvent(w));
            } catch (Throwable ex) {
                Bridge.log(ex, "a world init event");
            }
        }
        for (FerraloomWorld w : server.worlds) {
            try {
                server.pluginManager.callEvent(new WorldLoadEvent(w));
            } catch (Throwable ex) {
                Bridge.log(ex, "a world load event");
            }
        }
    }
}
