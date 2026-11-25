package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubServer;
import java.io.File;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Properties;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.logging.Level;
import java.util.logging.Logger;
import net.md_5.bungee.api.chat.BaseComponent;
import net.md_5.bungee.api.chat.TextComponent;
import org.bukkit.ChatColor;
import org.bukkit.UnsafeValues;
import org.bukkit.Warning.WarningState;
import org.bukkit.World;
import org.bukkit.command.Command;
import org.bukkit.command.CommandException;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.command.PluginCommand;
import org.bukkit.command.SimpleCommandMap;
import org.bukkit.entity.Player;
import org.bukkit.permissions.Permissible;
import org.bukkit.permissions.Permission;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginLoadOrder;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.ServicesManager;
import org.bukkit.plugin.SimplePluginManager;
import org.bukkit.plugin.SimpleServicesManager;
import org.bukkit.plugin.java.JavaPluginLoader;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.plugin.messaging.StandardMessenger;
import org.bukkit.scheduler.BukkitScheduler;
import org.bukkit.util.permissions.DefaultPermissions;

/**
 * {@link org.bukkit.Server} for Ferraloom: Bukkit's own plugin manager,
 * command map, services and messenger from the API jar, over the Rust
 * server's state through the natives ({@link ServerNative},
 * {@link PlayerNative}, {@link WorldNative}). What is not implemented yet
 * throws from {@link StubServer} (generated, see tools/StubGen.java).
 */
final class FerraloomServer extends StubServer {
    final Logger logger;
    final File directory;
    final String version;
    final String bukkitVersion;
    final int maxPlayers;
    final int port;
    final String motd;
    final boolean onlineMode;
    final List<FerraloomWorld> worlds;
    final FerraloomCommandMap commandMap;
    final SimplePluginManager pluginManager;
    final FerraloomScheduler scheduler;
    final ServicesManager services = new SimpleServicesManager();
    final Messenger messenger = new StandardMessenger();
    final FerraloomConsole console = new FerraloomConsole(this);
    final FerraloomUnsafe unsafe = new FerraloomUnsafe();
    final Thread primaryThread = Thread.currentThread();
    /** Online players in join order, and by UUID. Changed on the server thread only. */
    private final List<FerraloomPlayer> online = new CopyOnWriteArrayList<>();
    private final Map<UUID, FerraloomPlayer> byId = new ConcurrentHashMap<>();
    private final Spigot spigot = new Spigot() {
        @Override
        public void broadcast(BaseComponent component) {
            broadcastMessage(component.toLegacyText());
        }

        @Override
        public void broadcast(BaseComponent... components) {
            broadcastMessage(TextComponent.toLegacyText(components));
        }
    };

    FerraloomServer(Logger logger, File directory, String version, String[] worldNames, int[] environments,
            int maxPlayers, int port, String motd, boolean onlineMode) {
        this.logger = logger;
        this.directory = directory;
        this.version = version;
        this.bukkitVersion = apiVersion();
        this.maxPlayers = maxPlayers;
        this.port = port;
        this.motd = motd;
        this.onlineMode = onlineMode;
        List<FerraloomWorld> list = new ArrayList<>();
        for (int i = 0; i < worldNames.length; i++) {
            list.add(new FerraloomWorld(this, i, worldNames[i], World.Environment.values()[environments[i]]));
        }
        this.worlds = List.copyOf(list);
        this.commandMap = new FerraloomCommandMap(this);
        this.pluginManager = new SimplePluginManager(this, commandMap);
        this.scheduler = new FerraloomScheduler();
    }

    /** The API jar's own version (CraftServer reads the same file): {@code 26.3-R0.1-SNAPSHOT}. */
    private static String apiVersion() {
        try (InputStream in = org.bukkit.Bukkit.class.getResourceAsStream("/META-INF/maven/org.spigotmc/spigot-api/pom.properties")) {
            if (in != null) {
                Properties p = new Properties();
                p.load(in);
                return p.getProperty("version", "unknown");
            }
        } catch (Exception ignored) {
            // Falls through to "unknown".
        }
        return "unknown";
    }

    // Startup and shutdown, in CraftServer's order.

    /** CraftServer.loadPlugins: every jar in plugins/ that needs no server internals, then their onLoad. */
    void loadPlugins() {
        pluginManager.registerInterface(JavaPluginLoader.class);
        File folder = new File(directory, "plugins");
        File[] jars = folder.listFiles(f -> f.isFile() && f.getName().endsWith(".jar"));
        if (jars == null) {
            return;
        }
        java.util.Arrays.sort(jars);
        List<File> loadable = new ArrayList<>();
        for (File jar : jars) {
            String internal = Internals.find(jar);
            if (internal == null) {
                loadable.add(jar);
            } else {
                logger.severe("Could not load 'plugins/" + jar.getName() + "': it uses " + internal
                        + ", a Minecraft server internal (NMS or CraftBukkit) that Ferraloom does not provide");
            }
        }
        for (Plugin plugin : pluginManager.loadPlugins(loadable.toArray(new File[0]))) {
            try {
                plugin.getLogger().info(String.format("Loading %s", plugin.getDescription().getFullName()));
                plugin.onLoad();
            } catch (Throwable ex) {
                logger.log(Level.SEVERE, ex.getMessage() + " initializing " + plugin.getDescription().getFullName()
                        + " (Is it up to date?)", ex);
            }
        }
    }

    /** CraftServer.enablePlugins for both load orders (the worlds are loaded by then), then the core permissions. */
    void enablePlugins() {
        for (PluginLoadOrder order : PluginLoadOrder.values()) {
            for (Plugin plugin : pluginManager.getPlugins()) {
                if (!plugin.isEnabled() && plugin.getDescription().getLoad() == order) {
                    enablePlugin(plugin);
                }
            }
            if (order == PluginLoadOrder.STARTUP) {
                WorldEvents.loaded(this); // Ferraloom: worlds area, WorldInitEvent and WorldLoadEvent.
            }
        }
        DefaultPermissions.registerCorePermissions();
    }

    /** CraftServer.enablePlugin: its plugin.yml permissions, then the plugin manager's enable. */
    private void enablePlugin(Plugin plugin) {
        try {
            for (Permission perm : plugin.getDescription().getPermissions()) {
                try {
                    pluginManager.addPermission(perm, false);
                } catch (IllegalArgumentException ex) {
                    logger.log(Level.WARNING, "Plugin " + plugin.getDescription().getFullName()
                            + " tried to register permission '" + perm.getName() + "' but it's already registered", ex);
                }
            }
            pluginManager.dirtyPermissibles();
            pluginManager.enablePlugin(plugin);
        } catch (Throwable ex) {
            logger.log(Level.SEVERE, ex.getMessage() + " loading " + plugin.getDescription().getFullName()
                    + " (Is it up to date?)", ex);
        }
    }

    void disablePlugins() {
        pluginManager.disablePlugins();
        scheduler.shutdown();
    }

    // Players.

    /** A player's object (made at login, see PlayerEvents.login) goes online. */
    void join(FerraloomPlayer player) {
        online.add(player);
        byId.put(player.id, player);
    }

    void leave(FerraloomPlayer player) {
        player.online = false;
        online.remove(player);
        byId.remove(player.id, player);
    }

    FerraloomPlayer online(UUID id) {
        return byId.get(id);
    }

    FerraloomWorld world(int index) {
        return index >= 0 && index < worlds.size() ? worlds.get(index) : null;
    }

    /** CraftServer.dispatchCommand for a plugin command: true when the command map has the label. */
    boolean dispatchPluginCommand(CommandSender sender, String line) {
        String label = line.split(" ", -1)[0].toLowerCase(Locale.ROOT);
        if (commandMap.getCommand(label) == null) {
            return false;
        }
        try {
            commandMap.dispatch(sender, line);
        } catch (CommandException ex) {
            sender.sendMessage(ChatColor.RED + "An internal error occurred while attempting to perform this command");
            logger.log(Level.SEVERE, null, ex);
        }
        return true;
    }

    // org.bukkit.Server.

    @Override
    public String getName() {
        return "Ferraloom";
    }

    @Override
    public String getVersion() {
        return version;
    }

    @Override
    public String getBukkitVersion() {
        return bukkitVersion;
    }

    @Override
    public Collection<? extends Player> getOnlinePlayers() {
        return java.util.Collections.unmodifiableList(online);
    }

    @Override
    public int getMaxPlayers() {
        return maxPlayers;
    }

    @Override
    public int getPort() {
        return port;
    }

    @Override
    public String getIp() {
        return "";
    }

    @Override
    public String getMotd() {
        return motd;
    }

    @Override
    public boolean getOnlineMode() {
        return onlineMode;
    }

    @Override
    public String getUpdateFolder() {
        return "update";
    }

    @Override
    public File getUpdateFolderFile() {
        return new File(new File(directory, "plugins"), getUpdateFolder());
    }

    @Override
    public File getWorldContainer() {
        return directory;
    }

    @Override
    public List<World> getWorlds() {
        return List.copyOf(worlds);
    }

    @Override
    public World getWorld(String name) {
        for (FerraloomWorld w : worlds) {
            if (w.name.equalsIgnoreCase(name)) {
                return w;
            }
        }
        return null;
    }

    @Override
    public World getWorld(UUID uid) {
        for (FerraloomWorld w : worlds) {
            if (w.uid.equals(uid)) {
                return w;
            }
        }
        return null;
    }

    @Override
    public Player getPlayer(UUID id) {
        return byId.get(id);
    }

    /** CraftServer.getPlayer(String): the exact name, else the only online name it starts. */
    @Override
    public Player getPlayer(String name) {
        Player exact = getPlayerExact(name);
        if (exact != null) {
            return exact;
        }
        String lower = name.toLowerCase(Locale.ROOT);
        Player found = null;
        int delta = Integer.MAX_VALUE;
        for (FerraloomPlayer p : online) {
            if (p.name.toLowerCase(Locale.ROOT).startsWith(lower)) {
                int d = Math.abs(p.name.length() - lower.length());
                if (d < delta) {
                    found = p;
                    delta = d;
                }
                if (d == 0) {
                    break;
                }
            }
        }
        return found;
    }

    @Override
    public Player getPlayerExact(String name) {
        for (FerraloomPlayer p : online) {
            if (p.name.equalsIgnoreCase(name)) {
                return p;
            }
        }
        return null;
    }

    @Override
    public Logger getLogger() {
        return logger;
    }

    @Override
    public PluginManager getPluginManager() {
        return pluginManager;
    }

    @Override
    public BukkitScheduler getScheduler() {
        return scheduler;
    }

    @Override
    public ServicesManager getServicesManager() {
        return services;
    }

    @Override
    public Messenger getMessenger() {
        return messenger;
    }

    @Override
    public ConsoleCommandSender getConsoleSender() {
        return console;
    }

    @Override
    public PluginCommand getPluginCommand(String name) {
        Command command = commandMap.getCommand(name);
        return command instanceof PluginCommand pc ? pc : null;
    }

    /** CraftServer.dispatchCommand: a plugin's command, else vanilla's (run as the console). */
    @Override
    public boolean dispatchCommand(CommandSender sender, String commandLine) {
        if (!isPrimaryThread()) {
            throw new IllegalStateException("Asynchronous command dispatch!");
        }
        if (dispatchPluginCommand(sender, commandLine)) {
            return true;
        }
        if (sender instanceof ConsoleCommandSender) {
            return ServerNative.dispatchVanilla(commandLine);
        }
        throw new UnsupportedOperationException(
                "Ferraloom does not implement Server.dispatchCommand(CommandSender, String) yet for vanilla commands run by players");
    }

    @Override
    public int broadcastMessage(String message) {
        return broadcast(message, BROADCAST_CHANNEL_USERS);
    }

    /** CraftServer.broadcast: everyone subscribed to the permission, the console among them. */
    @Override
    public int broadcast(String message, String permission) {
        java.util.Set<CommandSender> recipients = new java.util.LinkedHashSet<>();
        for (Permissible permissible : pluginManager.getPermissionSubscriptions(permission)) {
            if (permissible instanceof CommandSender sender && permissible.hasPermission(permission)) {
                recipients.add(sender);
            }
        }
        for (CommandSender sender : recipients) {
            sender.sendMessage(message);
        }
        return recipients.size();
    }

    @Override
    public boolean isPrimaryThread() {
        return Thread.currentThread() == primaryThread;
    }

    @Override
    public WarningState getWarningState() {
        return WarningState.DEFAULT;
    }

    @Override
    public UnsafeValues getUnsafe() {
        return unsafe;
    }

    @Override public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(Class<T> type) { return FerraloomRegistries.get(type); } // Ferraloom: registries (blocks-worlds area, FerraloomRegistries)
    // Blocks area: BlockData (Blocks).
    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material material) { return Blocks.parse(material, null); }
    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material material, java.util.function.Consumer<? super org.bukkit.block.data.BlockData> consumer) { org.bukkit.block.data.BlockData data = createBlockData(material); if (consumer != null) { consumer.accept(data); } return data; }
    @Override public org.bukkit.block.data.BlockData createBlockData(String data) { return Blocks.parse(null, data); }
    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material material, String data) { return Blocks.parse(material, data); }

    @Override
    public void shutdown() {
        ServerNative.shutdown();
    }

    @Override
    public void savePlayers() {
        // The server saves its players itself.
    }

    @Override
    public Spigot spigot() {
        return spigot;
    }
}
