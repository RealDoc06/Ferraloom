package dev.ferraloom.testplugin;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.GameRule;
import org.bukkit.HeightMap;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.block.Block;
import org.bukkit.block.data.BlockData;
import org.bukkit.block.data.Orientable;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.block.BlockPhysicsEvent;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.inventory.InventoryClickEvent;
import org.bukkit.event.inventory.InventoryCloseEvent;
import org.bukkit.event.inventory.InventoryOpenEvent;
import org.bukkit.event.weather.WeatherChangeEvent;
import org.bukkit.event.world.WorldInitEvent;
import org.bukkit.event.world.WorldLoadEvent;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Exercises the bridge's vertical slice and logs what it sees as
 * {@code [FerraloomTest] <kind>: <what>} lines that tools/plugins_e2e.py checks.
 */
public final class TestPlugin extends JavaPlugin implements Listener {
    private volatile boolean protect;
    /** The next BlockPhysicsEvent is logged (then this goes false again). */
    private boolean watchPhysics;
    private int timerRuns;

    @Override
    public void onLoad() {
        getLogger().info("lifecycle: onLoad");
    }

    @Override
    public void onEnable() {
        // Entities: their checks and /fentity.
        EntityChecks entities = new EntityChecks(this);
        getServer().getPluginManager().registerEvents(entities, this);
        getCommand("fentity").setExecutor(entities);
        getLogger().info("lifecycle: onEnable");
        saveDefaultConfig();
        getLogger().info("config: greeting=" + getConfig().getString("greeting")
                + " folder=" + getDataFolder().isDirectory());
        List<String> worlds = new ArrayList<>();
        for (World w : Bukkit.getWorlds()) {
            worlds.add(w.getName() + "/" + w.getEnvironment());
        }
        getLogger().info("server: name=" + Bukkit.getName() + " bukkit=" + Bukkit.getBukkitVersion()
                + " online=" + Bukkit.getOnlinePlayers().size() + " worlds=" + String.join(",", worlds)
                + " primary=" + Bukkit.isPrimaryThread());
        getServer().getPluginManager().registerEvents(this, this);
        World world = Bukkit.getWorlds().get(0);
        // From a task, so both readings of the clock are at the start of a tick.
        getServer().getScheduler().runTask(this, () -> {
            long start = world.getFullTime();
            getServer().getScheduler().runTaskLater(this,
                    () -> getLogger().info("schedule: later after " + (world.getFullTime() - start) + " ticks"), 40);
            getServer().getScheduler().runTaskTimer(this, task -> {
                timerRuns++;
                if (timerRuns == 5) {
                    getLogger().info("schedule: timer ran 5 times in " + (world.getFullTime() - start) + " ticks");
                    task.cancel();
                }
            }, 0, 10);
        });
        getServer().getScheduler().runTaskAsynchronously(this,
                () -> getLogger().info("schedule: async primary=" + Bukkit.isPrimaryThread()));
        // Block data is static: readable off the server thread, as on CraftBukkit.
        getServer().getScheduler().runTaskAsynchronously(this, () -> getLogger().info("blocks: async primary="
                + Bukkit.isPrimaryThread() + " air=" + org.bukkit.Material.AIR.isAir() + " solid="
                + org.bukkit.Material.STONE.isSolid() + " block=" + org.bukkit.Material.OAK_LOG.isBlock() + " data="
                + org.bukkit.Material.OAK_LOG.createBlockData().getAsString() + " parsed="
                + Bukkit.createBlockData("minecraft:oak_stairs[facing=east]").getAsString(true)));
    }

    @Override
    public void onDisable() {
        getLogger().info("lifecycle: onDisable");
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player p = event.getPlayer();
        getLogger().info("event: join " + p.getName() + " " + p.getUniqueId() + " online=" + Bukkit.getOnlinePlayers().size());
        event.setJoinMessage("§a" + p.getName() + " joined through FerraloomTest");
        p.sendMessage("FerraloomTest welcomes " + p.getName());
    }

    @EventHandler
    public void onQuit(PlayerQuitEvent event) {
        getLogger().info("event: quit " + event.getPlayer().getName() + " online=" + event.getPlayer().isOnline());
        event.setQuitMessage(null);
    }

    @EventHandler(priority = EventPriority.LOW)
    public void onChatLow(AsyncPlayerChatEvent event) {
        getLogger().info("event: chat-low async=" + event.isAsynchronous() + " primary=" + Bukkit.isPrimaryThread());
        event.setFormat("[test] <%1$s> %2$s");
        if (event.getMessage().contains("secret")) {
            event.setCancelled(true);
        }
    }

    @EventHandler(priority = EventPriority.HIGH, ignoreCancelled = true)
    public void onChatHigh(AsyncPlayerChatEvent event) {
        getLogger().info("event: chat-high " + event.getMessage());
    }

    @EventHandler(priority = EventPriority.MONITOR)
    public void onChatMonitor(AsyncPlayerChatEvent event) {
        getLogger().info("event: chat-monitor cancelled=" + event.isCancelled());
    }

    @EventHandler
    public void onBreak(BlockBreakEvent event) {
        event.setCancelled(protect);
        getLogger().info("event: break " + event.getBlock().getType() + " at " + event.getBlock().getX() + " "
                + event.getBlock().getY() + " " + event.getBlock().getZ() + " by " + event.getPlayer().getName()
                + " cancelled=" + event.isCancelled());
    }

    /** The chest GUI's holder, as GUI plugins mark their inventories. */
    private static final class Gui implements InventoryHolder {
        private Inventory inventory;

        @Override
        public Inventory getInventory() {
            return inventory;
        }
    }

    /** A 27-slot GUI: a priced emerald in the middle (clicks on it are cancelled) and two free diamonds. */
    private void openGui(Player p) {
        Gui gui = new Gui();
        gui.inventory = Bukkit.createInventory(gui, 27, "§6Ferraloom GUI");
        ItemStack buy = new ItemStack(Material.EMERALD);
        ItemMeta meta = buy.getItemMeta();
        meta.setDisplayName("§aBuy");
        meta.setLore(List.of("§7Price: 5"));
        meta.getPersistentDataContainer().set(new NamespacedKey(this, "price"), PersistentDataType.INTEGER, 5);
        buy.setItemMeta(meta);
        gui.inventory.setItem(13, buy);
        gui.inventory.setItem(0, new ItemStack(Material.DIAMOND, 2));
        p.openInventory(gui.inventory);
    }

    @EventHandler
    public void onOpen(InventoryOpenEvent event) {
        getLogger().info("event: open " + event.getView().getTitle() + " gui=" + (event.getInventory().getHolder() instanceof Gui));
    }

    @EventHandler
    public void onClick(InventoryClickEvent event) {
        if (!(event.getView().getTopInventory().getHolder() instanceof Gui)) {
            getLogger().info("event: click " + event.getView().getType() + " raw=" + event.getRawSlot() + " " + event.getClick()
                    + " " + event.getAction());
            return;
        }
        ItemStack current = event.getCurrentItem();
        Integer price = current == null || !current.hasItemMeta() ? null
                : current.getItemMeta().getPersistentDataContainer().get(new NamespacedKey(this, "price"), PersistentDataType.INTEGER);
        event.setCancelled(event.getRawSlot() == 13);
        getLogger().info("event: click gui raw=" + event.getRawSlot() + " " + event.getClick() + " " + event.getAction()
                + " " + event.getSlotType() + " current=" + (current == null ? null : current.getType())
                + " price=" + price + " title=" + event.getView().getTitle() + " cancelled=" + event.isCancelled());
    }

    @EventHandler
    public void onClose(InventoryCloseEvent event) {
        getLogger().info("event: close " + event.getView().getTitle() + " gui=" + (event.getInventory().getHolder() instanceof Gui));
    }

    @EventHandler
    public void onPlace(BlockPlaceEvent event) {
        event.setCancelled(protect);
        Block b = event.getBlockPlaced();
        getLogger().info("event: place " + b.getType() + " at " + b.getX() + " " + b.getY() + " " + b.getZ()
                + " against " + event.getBlockAgainst().getType() + " replacing " + event.getBlockReplacedState().getType()
                + " by " + event.getPlayer().getName() + " cancelled=" + event.isCancelled());
    }

    @EventHandler
    public void onPhysics(BlockPhysicsEvent event) {
        if (watchPhysics) {
            watchPhysics = false;
            Block b = event.getBlock();
            getLogger().info("event: physics at " + b.getX() + " " + b.getY() + " " + b.getZ() + " changed="
                    + event.getChangedType());
        }
    }

    @EventHandler
    public void onWeather(WeatherChangeEvent event) {
        getLogger().info("event: weather " + event.getWorld().getName() + " to=" + event.toWeatherState());
    }

    @EventHandler
    public void onWorldInit(WorldInitEvent event) {
        getLogger().info("event: world-init " + event.getWorld().getName());
    }

    @EventHandler
    public void onWorldLoad(WorldLoadEvent event) {
        getLogger().info("event: world-load " + event.getWorld().getName());
    }

    private static int arg(String[] args, int i) {
        return Integer.parseInt(args[i]);
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        String sub = args.length == 0 ? "" : args[0].toLowerCase(Locale.ROOT);
        switch (sub) {
            case "ping" -> sender.sendMessage("pong " + sender.getName());
            case "protect" -> {
                protect = args.length > 1 && args[1].equals("on");
                sender.sendMessage("protect " + protect);
            }
            case "where" -> {
                if (sender instanceof Player p) {
                    Location at = p.getLocation();
                    sender.sendMessage(String.format(Locale.ROOT, "at %s %.1f %.1f %.1f", at.getWorld().getName(),
                            at.getX(), at.getY(), at.getZ()));
                }
            }
            case "tp" -> {
                if (sender instanceof Player p && args.length == 4) {
                    Location to = new Location(p.getWorld(), Double.parseDouble(args[1]), Double.parseDouble(args[2]),
                            Double.parseDouble(args[3]));
                    sender.sendMessage("teleported " + p.teleport(to));
                }
            }
            case "stub" -> sender.sendMessage("health " + ((Player) sender).getHealth());
            case "gui" -> openGui((Player) sender);
            case "cursor" -> {
                ItemStack cursor = ((Player) sender).getItemOnCursor();
                sender.sendMessage("cursor " + cursor.getType() + " " + cursor.getAmount());
            }
            case "hand" -> {
                // A slot's item writes through to the slot, as CraftItemStack's mirror does.
                Player p = (Player) sender;
                p.getInventory().setItem(p.getInventory().getHeldItemSlot(), new ItemStack(Material.GOLD_INGOT, 5));
                ItemStack hand = p.getInventory().getItemInMainHand();
                hand.setAmount(hand.getAmount() - 2);
                sender.sendMessage("hand " + p.getInventory().getItemInMainHand().getAmount() + " "
                        + p.getInventory().contains(Material.GOLD_INGOT, 3));
            }
            case "block" -> {
                World w = Bukkit.getWorlds().get(0);
                Block b = w.getBlockAt(arg(args, 1), arg(args, 2), arg(args, 3));
                Block up = b.getRelative(org.bukkit.block.BlockFace.UP);
                sender.sendMessage("block " + b.getType() + " " + b.getBlockData().getAsString() + " solid="
                        + b.getType().isSolid() + " empty=" + b.isEmpty() + " above=" + up.getType() + " sky="
                        + up.getLightFromSky() + " biome=" + b.getBiome().getKey());
            }
            case "set" -> {
                World w = Bukkit.getWorlds().get(0);
                BlockData data = Bukkit.createBlockData(args[4]);
                if (data instanceof Orientable o) {
                    o.setAxis(org.bukkit.Axis.X);
                }
                w.getBlockAt(arg(args, 1), arg(args, 2), arg(args, 3)).setBlockData(data);
                sender.sendMessage("set " + w.getBlockAt(arg(args, 1), arg(args, 2), arg(args, 3)).getBlockData().getAsString());
            }
            case "world" -> {
                World w = Bukkit.getWorlds().get(0);
                Location at = sender instanceof Player p ? p.getLocation() : w.getSpawnLocation();
                int cx = at.getBlockX() >> 4;
                int cz = at.getBlockZ() >> 4;
                sender.sendMessage("world sea=" + w.getSeaLevel() + " highest="
                        + w.getHighestBlockYAt(at.getBlockX(), at.getBlockZ(), HeightMap.MOTION_BLOCKING)
                        + " loaded=" + w.isChunkLoaded(cx, cz) + " generated=" + w.isChunkGenerated(cx, cz)
                        + " far=" + w.isChunkLoaded(cx + 1000, cz) + " border=" + w.getWorldBorder().getSize()
                        + " inside=" + w.getWorldBorder().isInside(at) + " keepInventory="
                        + w.getGameRuleValue(GameRule.KEEP_INVENTORY) + " nether=" + Bukkit.getWorlds().get(1).getBlockAt(0, 0, 0).getType());
            }
            case "storm" -> {
                World w = Bukkit.getWorlds().get(0);
                w.setStorm(args.length > 1 && args[1].equals("on"));
                sender.sendMessage("storm " + w.hasStorm());
            }
            case "sign" -> {
                Block b = Bukkit.getWorlds().get(0).getBlockAt(arg(args, 1), arg(args, 2), arg(args, 3));
                b.setType(org.bukkit.Material.OAK_SIGN);
                if (b.getState() instanceof org.bukkit.block.Sign sign) {
                    sign.setLine(0, "hello");
                    sign.update();
                }
                String line = b.getState() instanceof org.bukkit.block.Sign again ? again.getLine(0) : "no sign";
                sender.sendMessage("sign " + line);
            }
            case "physics" -> {
                watchPhysics = true;
                sender.sendMessage("watching physics");
            }
            default -> {
                return false;
            }
        }
        getLogger().info("command: " + sender.getName() + " ran " + label + " " + String.join(" ", args));
        return true;
    }
}
