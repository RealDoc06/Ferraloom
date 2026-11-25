package dev.ferraloom.bukkit;

import java.util.LinkedHashSet;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.bukkit.Location;
import org.bukkit.entity.Player;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;

/**
 * The players area's event entry points (crates/ferraloom-plugins/src/players.rs).
 * All but {@link #chat}'s event run on the server thread.
 */
public final class PlayerEvents {
    /** AsyncPlayerChatEvent's thread: one, so a player's lines keep their order. */
    private static final ExecutorService CHAT = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "Async Chat Thread");
        t.setDaemon(true);
        return t;
    });

    private PlayerEvents() {
    }

    static void stop() throws InterruptedException {
        CHAT.shutdown();
        CHAT.awaitTermination(5, TimeUnit.SECONDS);
    }

    /** PlayerList.placeNewPlayer's PlayerJoinEvent: the join message to send, or null for none. */
    public static String join(long most, long least, String name, int entityId, boolean op, String message) {
        try {
            FerraloomPlayer player = Bridge.server.join(new UUID(most, least), name, entityId, op);
            PlayerJoinEvent event = new PlayerJoinEvent(player, message);
            Bridge.server.pluginManager.callEvent(event);
            return event.getJoinMessage();
        } catch (Throwable ex) {
            Bridge.log(ex, "a player joining");
            return message;
        }
    }

    /** PlayerList.remove's PlayerQuitEvent, the player still online during it: the quit message, or null. */
    public static String quit(long most, long least, String message, int world, double x, double y, double z, float yaw,
            float pitch) {
        FerraloomServer server = Bridge.server;
        FerraloomPlayer player = server.online(new UUID(most, least));
        if (player == null) {
            return message;
        }
        try {
            player.leaving(new Location(server.world(world), x, y, z, yaw, pitch));
            PlayerQuitEvent event = new PlayerQuitEvent(player, message);
            server.pluginManager.callEvent(event);
            return event.getQuitMessage();
        } catch (Throwable ex) {
            Bridge.log(ex, "a player leaving");
            return message;
        } finally {
            server.leave(player);
        }
    }

    /**
     * A player's chat line, for AsyncPlayerChatEvent on the chat thread. The
     * line reaches its recipients as the event leaves it, at the start of the
     * server's next tick (messages from other threads are queued until then).
     */
    public static void chat(long most, long least, String message) {
        FerraloomServer server = Bridge.server;
        FerraloomPlayer player = server.online(new UUID(most, least));
        if (player == null) {
            return;
        }
        CHAT.execute(() -> {
            try {
                Set<Player> recipients = new LinkedHashSet<>(server.getOnlinePlayers());
                AsyncPlayerChatEvent event = new AsyncPlayerChatEvent(true, player, message, recipients);
                server.pluginManager.callEvent(event);
                if (event.isCancelled()) {
                    return;
                }
                String line = String.format(event.getFormat(), event.getPlayer().getDisplayName(), event.getMessage());
                server.console.sendMessage(line);
                for (Player recipient : event.getRecipients()) {
                    recipient.sendMessage(line);
                }
            } catch (Throwable ex) {
                Bridge.log(ex, "a chat message");
            }
        });
    }

    /** A command line (no slash) a player typed: true when a plugin's command took it. */
    public static boolean command(long most, long least, String line) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null) {
            return false;
        }
        try {
            return Bridge.server.dispatchPluginCommand(player, line);
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's command");
            return true;
        }
    }
}
