package dev.ferraloom.testplugin;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import org.bukkit.BanList;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.OfflinePlayer;
import org.bukkit.World;
import org.bukkit.ban.ProfileBanList;
import org.bukkit.boss.BarColor;
import org.bukkit.boss.BarStyle;
import org.bukkit.boss.BossBar;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.event.server.TabCompleteEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Exercises the bridge's vertical slice and logs what it sees as
 * {@code [FerraloomTest] <kind>: <what>} lines that tools/plugins_e2e.py checks.
 */
public final class TestPlugin extends JavaPlugin implements Listener {
    private static final List<String> SUBCOMMANDS =
            List.of("ping", "protect", "where", "tp", "stub", "lists", "pardon", "bar", "stall");
    private volatile boolean protect;
    private int timerRuns;
    private BossBar bar;

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

    @EventHandler
    public void onTabComplete(TabCompleteEvent event) {
        getLogger().info("event: tab " + event.getBuffer() + " -> " + event.getCompletions());
    }

    /** The first argument's subcommands; later ones fall back to Bukkit's default (online player names). */
    @Override
    public List<String> onTabComplete(CommandSender sender, Command command, String alias, String[] args) {
        if (args.length != 1) {
            return null;
        }
        return SUBCOMMANDS.stream().filter(s -> s.startsWith(args[0].toLowerCase(Locale.ROOT))).toList();
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (command.getName().equals("fadmin")) {
            sender.sendMessage("admin ok");
            return true;
        }
        String sub = args.length == 0 ? "" : args[0].toLowerCase(Locale.ROOT);
        switch (sub) {
            case "lists" -> {
                OfflinePlayer o = getServer().getOfflinePlayer(args[1]);
                UUID offline = UUID.nameUUIDFromBytes(("OfflinePlayer:" + args[1]).getBytes(StandardCharsets.UTF_8));
                boolean isOffline = !o.isOnline() && o.getUniqueId().equals(offline);
                o.setOp(true);
                boolean op = o.isOp() && getServer().getOperators().contains(o);
                o.setOp(false);
                o.setWhitelisted(true);
                boolean white = o.isWhitelisted() && getServer().getWhitelistedPlayers().contains(o);
                o.setWhitelisted(false);
                ProfileBanList bans = getServer().getBanList(BanList.Type.PROFILE);
                bans.addBan(o.getPlayerProfile(), "testing bans", (Date) null, "FerraloomTest");
                sender.sendMessage("lists: offline=" + isOffline + " op=" + op + "," + o.isOp() + " whitelist=" + white + ","
                        + o.isWhitelisted() + " banned=" + o.isBanned());
            }
            case "pardon" -> {
                OfflinePlayer o = getServer().getOfflinePlayer(args[1]);
                ProfileBanList bans = getServer().getBanList(BanList.Type.PROFILE);
                bans.pardon(o.getPlayerProfile());
                sender.sendMessage("pardon: banned=" + o.isBanned());
            }
            case "bar" -> {
                if (args.length > 1 && args[1].equals("off")) {
                    bar.removeAll();
                } else if (sender instanceof Player p) {
                    bar = getServer().createBossBar("§cFerraloom bar", BarColor.RED, BarStyle.SOLID);
                    bar.setProgress(0.5);
                    bar.addPlayer(p);
                }
                sender.sendMessage("bar " + (bar == null ? 0 : bar.getPlayers().size()));
            }
            case "stall" -> {
                try {
                    Thread.sleep(Long.parseLong(args[1]) * 1000);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
                sender.sendMessage("stalled");
            }
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
            default -> {
                return false;
            }
        }
        getLogger().info("command: " + sender.getName() + " ran " + label + " " + String.join(" ", args));
        return true;
    }
}
