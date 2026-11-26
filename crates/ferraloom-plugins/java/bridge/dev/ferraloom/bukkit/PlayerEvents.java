package dev.ferraloom.bukkit;

import java.net.InetAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.bukkit.GameMode;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.damage.DamageSource;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.block.Action;
import org.bukkit.event.entity.PlayerDeathEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.AsyncPlayerPreLoginEvent;
import org.bukkit.event.player.PlayerChangedWorldEvent;
import org.bukkit.event.player.PlayerCommandPreprocessEvent;
import org.bukkit.event.player.PlayerGameModeChangeEvent;
import org.bukkit.event.player.PlayerInteractEvent;
import org.bukkit.event.player.PlayerItemHeldEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerKickEvent;
import org.bukkit.event.player.PlayerLoginEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.event.player.PlayerRespawnEvent;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.event.player.PlayerTeleportEvent.TeleportCause;
import org.bukkit.event.player.PlayerToggleFlightEvent;
import org.bukkit.event.player.PlayerToggleSneakEvent;
import org.bukkit.event.player.PlayerToggleSprintEvent;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.messaging.ChannelNameTooLongException;
import org.bukkit.plugin.messaging.StandardMessenger;
import org.bukkit.util.Vector;

/**
 * The players area's event entry points (crates/ferraloom-plugins/src/players.rs, same names and signatures).
 * All run on the server thread but {@link #chat}'s event (the chat thread) and {@link #preLogin} (the login's
 * connection thread). Each is a wall: what escapes is logged and answered with the default (nothing changed).
 * Locations cross as (world index, x, y, z, yaw, pitch); a changed one comes back as a double[6], a cancelled
 * one as an empty array, an unchanged one as null.
 */
public final class PlayerEvents {
    /** AsyncPlayerChatEvent's thread: one, so a player's lines keep their order. */
    private static final ExecutorService CHAT = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "Async Chat Thread");
        t.setDaemon(true);
        return t;
    });
    /**
     * Players between PlayerLoginEvent and their join, whose join reuses the object (CraftBukkit does): a profile's
     * attempts in arrival order, since a later one is admitted only once the earlier ones are gone. Guarded by itself.
     */
    private static final Map<UUID, ArrayDeque<FerraloomPlayer>> LOGGING_IN = new HashMap<>();
    /** A pending player older than this lost its join (a path that never abandoned it): the next login drops it. */
    private static final long STALE_NANOS = TimeUnit.MINUTES.toNanos(5);
    private static final double[] CANCELLED = new double[0];
    /** CraftPlayer's limit on the channels a player registers. */
    private static final int MAX_CHANNELS = 128;
    /** Vanilla's Direction ids (down, up, north, south, west, east) as BlockFaces. */
    private static final BlockFace[] FACES = {BlockFace.DOWN, BlockFace.UP, BlockFace.NORTH, BlockFace.SOUTH,
            BlockFace.WEST, BlockFace.EAST};
    /** TeleportCause by the ordinals of src/players.rs's Cause. */
    private static final TeleportCause[] CAUSES = {TeleportCause.ENDER_PEARL, TeleportCause.COMMAND,
            TeleportCause.PLUGIN, TeleportCause.NETHER_PORTAL, TeleportCause.END_PORTAL, TeleportCause.SPECTATE,
            TeleportCause.END_GATEWAY, TeleportCause.CHORUS_FRUIT, TeleportCause.DISMOUNT, TeleportCause.EXIT_BED,
            TeleportCause.UNKNOWN};

    private PlayerEvents() {
    }

    static void stop() throws InterruptedException {
        CHAT.shutdown();
        CHAT.awaitTermination(5, TimeUnit.SECONDS);
    }

    private static FerraloomPlayer online(long most, long least) {
        return Bridge.server.online(new UUID(most, least));
    }

    private static Location location(int world, double x, double y, double z, float yaw, float pitch) {
        return new Location(Bridge.server.world(world), x, y, z, yaw, pitch);
    }

    /** A location's answer: null when it is `unchanged`, else {world, x, y, z, yaw, pitch}. */
    private static double[] answer(Location unchanged, Location to) {
        if (to == null) {
            return CANCELLED;
        }
        if (to.equals(unchanged)) {
            return null;
        }
        int world = to.getWorld() instanceof FerraloomWorld w ? w.index : -1;
        return world < 0 ? CANCELLED : new double[] {world, to.getX(), to.getY(), to.getZ(), to.getYaw(), to.getPitch()};
    }

    private static boolean call(Event event) {
        Bridge.server.pluginManager.callEvent(event);
        return event instanceof org.bukkit.event.Cancellable c && c.isCancelled();
    }

    // Login, join and quit.

    /** AsyncPlayerPreLoginEvent on the login's thread: the kick message, or null to go on. */
    public static String preLogin(long most, long least, String name, String ip, boolean transferred) {
        try {
            AsyncPlayerPreLoginEvent event = new AsyncPlayerPreLoginEvent(name, InetAddress.getByName(ip),
                    new UUID(most, least), transferred);
            Bridge.server.pluginManager.callEvent(event);
            if (event.getLoginResult() == AsyncPlayerPreLoginEvent.Result.ALLOWED) {
                return null;
            }
            return event.getKickMessage() == null ? "" : event.getKickMessage();
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's pre-login");
            return null;
        }
    }

    /**
     * PlayerList.canPlayerLogin's PlayerLoginEvent, with vanilla's verdict ({@code result}, PlayerLoginEvent.Result's
     * ordinal, and its message): the kick message, or null to let them in. Makes the player's object, pending
     * under {@code attempt} (src/players.rs's login attempt id) until its join.
     */
    public static String login(long most, long least, String name, String ip, String hostname, boolean op, int result,
            String message, long attempt) {
        UUID id = new UUID(most, least);
        try {
            FerraloomPlayer player = new FerraloomPlayer(Bridge.server, id, name, op);
            player.attempt = attempt;
            player.attemptNanos = System.nanoTime();
            synchronized (LOGGING_IN) {
                LOGGING_IN.values().forEach(q -> q.removeIf(p -> player.attemptNanos - p.attemptNanos > STALE_NANOS));
                LOGGING_IN.values().removeIf(ArrayDeque::isEmpty);
                LOGGING_IN.computeIfAbsent(id, k -> new ArrayDeque<>()).addLast(player);
            }
            InetAddress address = InetAddress.getByName(ip);
            PlayerLoginEvent event = new PlayerLoginEvent(player, hostname, address,
                    PlayerLoginEvent.Result.values()[result], message, address);
            Bridge.server.pluginManager.callEvent(event);
            if (event.getResult() == PlayerLoginEvent.Result.ALLOWED) {
                return null;
            }
            abandon(most, least, attempt);
            return event.getKickMessage() == null ? "" : event.getKickMessage();
        } catch (Throwable ex) {
            abandon(most, least, attempt);
            Bridge.log(ex, "a player's login");
            return result == 0 ? null : message;
        }
    }

    /** A login attempt that ended before its join: its pending player is dropped, not another attempt's. */
    public static void abandon(long most, long least, long attempt) {
        UUID id = new UUID(most, least);
        synchronized (LOGGING_IN) {
            ArrayDeque<FerraloomPlayer> pending = LOGGING_IN.get(id);
            if (pending != null && pending.removeIf(p -> p.attempt == attempt) && pending.isEmpty()) {
                LOGGING_IN.remove(id);
            }
        }
    }

    /** PlayerList.placeNewPlayer's PlayerJoinEvent: the join message to send, or null for none. */
    public static String join(long most, long least, String name, int entityId, boolean op, String message,
            boolean playedBefore, long firstPlayed, long lastPlayed) {
        try {
            UUID id = new UUID(most, least);
            FerraloomPlayer player = null;
            synchronized (LOGGING_IN) {
                ArrayDeque<FerraloomPlayer> pending = LOGGING_IN.get(id);
                if (pending != null) {
                    player = pending.pollFirst();
                    if (pending.isEmpty()) {
                        LOGGING_IN.remove(id);
                    }
                }
            }
            if (player == null) {
                player = new FerraloomPlayer(Bridge.server, id, name, op);
            }
            player.joined(entityId, playedBefore, firstPlayed, lastPlayed);
            Bridge.server.join(player);
            player.sendSupportedChannels();
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
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return message;
        }
        try {
            player.leaving(location(world, x, y, z, yaw, pitch));
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
     * ServerGamePacketListenerImpl.disconnect's PlayerKickEvent for a kick the server decided: {reason, leave
     * message}, or null when cancelled.
     */
    public static String[] kick(long most, long least, String reason, String leave) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return new String[] {reason, leave};
        }
        try {
            PlayerKickEvent event = kickEvent(player, reason);
            return event.isCancelled() ? null : new String[] {event.getReason(), event.getLeaveMessage()};
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's kick");
            return new String[] {reason, leave};
        }
    }

    private static PlayerKickEvent kickEvent(FerraloomPlayer player, String reason) {
        PlayerKickEvent event = new PlayerKickEvent(player, reason, "§e" + player.getName() + " left the game");
        Bridge.server.pluginManager.callEvent(event);
        return event;
    }

    /** Player.kickPlayer: the event, then the disconnect unless a listener cancelled it. */
    static void kick(FerraloomPlayer player, String reason) {
        if (!player.isOnline()) {
            return;
        }
        PlayerKickEvent event = kickEvent(player, reason);
        if (!event.isCancelled()) {
            PlayerNative.kick(player.id.getMostSignificantBits(), player.id.getLeastSignificantBits(),
                    event.getReason());
        }
    }

    // Chat and commands.

    /**
     * A player's chat line, for AsyncPlayerChatEvent on the chat thread. The line reaches its recipients as the
     * event leaves it, at the start of the server's next tick (messages from other threads are queued until then).
     */
    public static void chat(long most, long least, String message) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return;
        }
        CHAT.execute(() -> say(player, message, true));
    }

    /** ServerGamePacketListenerImpl.chat's AsyncPlayerChatEvent (async or, from Player.chat, not), then the lines. */
    static void say(FerraloomPlayer player, String message, boolean async) {
        FerraloomServer server = Bridge.server;
        try {
            Set<Player> recipients = new LinkedHashSet<>(server.getOnlinePlayers());
            AsyncPlayerChatEvent event = new AsyncPlayerChatEvent(async, player, message, recipients);
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
    }

    /** A command line (no slash) a player typed: true when a plugin's command took it. */
    public static boolean command(long most, long least, String line) {
        FerraloomPlayer player = online(most, least);
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

    /**
     * ServerGamePacketListenerImpl.handleCommand's PlayerCommandPreprocessEvent for a line (no slash): the line to
     * run, or null when cancelled or rewritten to nothing.
     */
    public static String preprocess(long most, long least, String line) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return line;
        }
        try {
            return preprocess(player, line);
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's command");
            return line;
        }
    }

    private static String preprocess(FerraloomPlayer player, String line) {
        Set<Player> recipients = new LinkedHashSet<>(Bridge.server.getOnlinePlayers());
        PlayerCommandPreprocessEvent event = new PlayerCommandPreprocessEvent(player, "/" + line, recipients);
        if (call(event)) {
            return null;
        }
        // A listener's line may come without its slash; an empty one runs nothing.
        String message = event.getMessage();
        String run = message.startsWith("/") ? message.substring(1) : message;
        return run.isEmpty() ? null : run;
    }

    /** Player.chat of a command: the preprocess event, then a plugin's command or vanilla's. */
    static void command(FerraloomPlayer player, String line) {
        String run = preprocess(player, line);
        if (run != null) {
            player.performCommand(run);
        }
    }

    // Movement.

    /** handleMovePlayer's PlayerMoveEvent, before the move: a changed destination goes through PlayerTeleportEvent. */
    public static double[] move(long most, long least, int fw, double fx, double fy, double fz, float fyaw, float fpitch,
            int tw, double tx, double ty, double tz, float tyaw, float tpitch) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return null;
        }
        try {
            Location from = location(fw, fx, fy, fz, fyaw, fpitch);
            Location to = location(tw, tx, ty, tz, tyaw, tpitch);
            PlayerMoveEvent event = new PlayerMoveEvent(player, from, to.clone());
            if (call(event)) {
                return CANCELLED;
            }
            if (to.equals(event.getTo())) {
                return null;
            }
            // CraftBukkit: a plugin's destination is a PLUGIN teleport.
            PlayerTeleportEvent teleport = new PlayerTeleportEvent(player, from, event.getTo(), TeleportCause.PLUGIN);
            if (call(teleport)) {
                return CANCELLED;
            }
            return answer(to, teleport.getTo());
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's move");
            return null;
        }
    }

    /** PlayerTeleportEvent for a teleport the server makes ({@code cause}: src/players.rs's Cause). */
    public static double[] teleport(long most, long least, int fw, double fx, double fy, double fz, float fyaw,
            float fpitch, int tw, double tx, double ty, double tz, float tyaw, float tpitch, int cause) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return null;
        }
        try {
            Location to = location(tw, tx, ty, tz, tyaw, tpitch);
            PlayerTeleportEvent event = new PlayerTeleportEvent(player, location(fw, fx, fy, fz, fyaw, fpitch),
                    to.clone(), CAUSES[cause]);
            if (call(event)) {
                return CANCELLED;
            }
            return answer(to, event.getTo());
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's teleport");
            return null;
        }
    }

    /** A PlayerToggleSneakEvent (0), PlayerToggleSprintEvent (1) or PlayerToggleFlightEvent (2): true when cancelled. */
    public static boolean toggle(long most, long least, int kind, boolean on) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return false;
        }
        try {
            return call(switch (kind) {
                case 0 -> new PlayerToggleSneakEvent(player, on);
                case 1 -> new PlayerToggleSprintEvent(player, on);
                default -> new PlayerToggleFlightEvent(player, on);
            });
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's toggle");
            return false;
        }
    }

    // Interaction.

    /**
     * PlayerInteractEvent. {@code world} is -1 without a block. Answers bit 1 for useInteractedBlock() == DENY and
     * bit 2 for useItemInHand() == DENY.
     */
    public static int interact(long most, long least, int action, byte[] item, int world, int x, int y, int z,
            int face, boolean offHand, double px, double py, double pz, boolean hasPoint) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return 0;
        }
        try {
            World w = world < 0 ? null : Bridge.server.world(world);
            Block block = w == null ? null : w.getBlockAt(x, y, z);
            BlockFace blockFace = block == null ? BlockFace.SELF : FACES[face];
            Vector point = hasPoint ? new Vector(px, py, pz) : null;
            PlayerInteractEvent event = new Interact(player, Action.values()[action], item, block, blockFace,
                    offHand ? EquipmentSlot.OFF_HAND : EquipmentSlot.HAND, point);
            Bridge.server.pluginManager.callEvent(event);
            return (event.useInteractedBlock() == Event.Result.DENY ? 1 : 0)
                    | (event.useItemInHand() == Event.Result.DENY ? 2 : 0);
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's interaction");
            return 0;
        }
    }

    /** PlayerInteractEvent whose item is read from its bytes when a listener first asks (Items.read allocates). */
    private static final class Interact extends PlayerInteractEvent {
        private final byte[] bytes;
        private ItemStack read;

        Interact(Player who, Action action, byte[] item, Block block, BlockFace face, EquipmentSlot hand,
                Vector point) {
            super(who, action, null, block, face, hand, point);
            this.bytes = item;
        }

        @Override
        public ItemStack getItem() {
            if (read == null && bytes != null) {
                read = Items.read(bytes);
            }
            return read;
        }

        @Override
        public Material getMaterial() {
            ItemStack item = getItem();
            return item == null ? Material.AIR : item.getType();
        }

        @Override
        public boolean hasItem() {
            return bytes != null;
        }

        @Override
        public boolean isBlockInHand() {
            return hasItem() && getMaterial().isBlock();
        }
    }

    /** handleSetCarriedItem's PlayerItemHeldEvent: true when cancelled. */
    public static boolean itemHeld(long most, long least, int previous, int now) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return false;
        }
        try {
            return call(new PlayerItemHeldEvent(player, previous, now));
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's held item");
            return false;
        }
    }

    // Life, death and worlds.

    /** PlayerList.respawn's PlayerRespawnEvent: where they respawn. */
    public static double[] respawn(long most, long least, int world, double x, double y, double z, float yaw,
            float pitch, boolean bed, boolean anchor) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return null;
        }
        try {
            Location at = location(world, x, y, z, yaw, pitch);
            PlayerRespawnEvent event = new PlayerRespawnEvent(player, at, bed, anchor,
                    PlayerRespawnEvent.RespawnReason.DEATH);
            Bridge.server.pluginManager.callEvent(event);
            double[] to = answer(null, event.getRespawnLocation());
            return to == null || to.length == 0 ? null : to;
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's respawn");
            return null;
        }
    }

    /**
     * ServerPlayer.die's PlayerDeathEvent: {death message, dropped experience, keepInventory, keepLevel}. The drops
     * and the damage source need the inventory and entities areas: they throw until then.
     */
    public static String[] death(long most, long least, String message, int droppedExp, int newExp,
            boolean keepInventory, boolean keepLevel) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return null;
        }
        try {
            PlayerDeathEvent event = new Death(player, droppedExp, newExp, message);
            event.setKeepInventory(keepInventory);
            event.setKeepLevel(keepLevel);
            Bridge.server.pluginManager.callEvent(event);
            return new String[] {event.getDeathMessage(), Integer.toString(event.getDroppedExp()),
                    Boolean.toString(event.getKeepInventory()), Boolean.toString(event.getKeepLevel())};
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's death");
            return null;
        }
    }

    private static final class Death extends PlayerDeathEvent {
        Death(Player player, int droppedExp, int newExp, String message) {
            super(player, null, new ArrayList<>(), droppedExp, newExp, message);
        }

        @Override
        public List<ItemStack> getDrops() {
            throw new UnsupportedOperationException("Ferraloom does not implement PlayerDeathEvent.getDrops() yet");
        }

        @Override
        public DamageSource getDamageSource() {
            throw new UnsupportedOperationException(
                    "Ferraloom does not implement PlayerDeathEvent.getDamageSource() yet");
        }
    }

    /** ServerPlayer.setGameMode's PlayerGameModeChangeEvent ({@code mode}: GameType's id): true when cancelled. */
    public static boolean gameModeChange(long most, long least, int mode) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return false;
        }
        try {
            GameMode to = switch (mode) {
                case 1 -> GameMode.CREATIVE;
                case 2 -> GameMode.ADVENTURE;
                case 3 -> GameMode.SPECTATOR;
                default -> GameMode.SURVIVAL;
            };
            return call(new PlayerGameModeChangeEvent(player, to));
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's game mode");
            return false;
        }
    }

    /** ServerPlayer.triggerDimensionChangeTriggers' PlayerChangedWorldEvent, once the player is there. */
    public static void changedWorld(long most, long least, int from) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return;
        }
        try {
            Bridge.server.pluginManager.callEvent(new PlayerChangedWorldEvent(player, Bridge.server.world(from)));
        } catch (Throwable ex) {
            Bridge.log(ex, "a player's change of world");
        }
    }

    // Plugin messages.

    /**
     * A custom payload from the player's client: minecraft:register and unregister change the channels it listens
     * on (CraftPlayer.addChannel), anything else goes to the plugins registered on its channel.
     */
    public static void pluginMessage(long most, long least, String channel, byte[] data) {
        FerraloomPlayer player = online(most, least);
        if (player == null) {
            return;
        }
        try {
            switch (channel) {
                case "minecraft:register", "minecraft:unregister" -> {
                    // As CraftPlayer.addChannel: at most MAX_CHANNELS, each a valid channel name (anything else ignored).
                    for (String name : new String(data, StandardCharsets.UTF_8).split("\0")) {
                        String valid;
                        try {
                            valid = StandardMessenger.validateAndCorrectChannel(name);
                        } catch (IllegalArgumentException | ChannelNameTooLongException invalid) {
                            continue;
                        }
                        if (!channel.endsWith(":register")) {
                            player.channels.remove(valid);
                        } else if (player.channels.size() < MAX_CHANNELS) {
                            player.channels.add(valid);
                        }
                    }
                }
                default -> Bridge.server.getMessenger().dispatchIncomingMessage(player, channel, data);
            }
        } catch (Throwable ex) {
            Bridge.log(ex, "a plugin message");
        }
    }
}
