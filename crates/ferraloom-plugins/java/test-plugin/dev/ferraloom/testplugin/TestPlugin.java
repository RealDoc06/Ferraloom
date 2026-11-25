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
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.inventory.InventoryClickEvent;
import org.bukkit.event.inventory.InventoryCloseEvent;
import org.bukkit.event.inventory.InventoryOpenEvent;
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
    private int timerRuns;

    @Override
    public void onLoad() {
        getLogger().info("lifecycle: onLoad");
    }

    @Override
    public void onEnable() {
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
            default -> {
                return false;
            }
        }
        getLogger().info("command: " + sender.getName() + " ran " + label + " " + String.join(" ", args));
        return true;
    }
}
