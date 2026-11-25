package dev.ferraloom.testplugin;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.nio.charset.StandardCharsets;
import net.md_5.bungee.api.ChatMessageType;
import net.md_5.bungee.api.chat.TextComponent;
import org.bukkit.Bukkit;
import org.bukkit.GameMode;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.AsyncPlayerPreLoginEvent;
import org.bukkit.event.player.PlayerCommandPreprocessEvent;
import org.bukkit.event.player.PlayerGameModeChangeEvent;
import org.bukkit.event.player.PlayerInteractEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerKickEvent;
import org.bukkit.event.player.PlayerLoginEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.event.player.PlayerToggleSneakEvent;
import org.bukkit.metadata.FixedMetadataValue;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Exercises the bridge's vertical slice and logs what it sees as
 * {@code [FerraloomTest] <kind>: <what>} lines that tools/plugins_e2e.py checks.
 */
public final class TestPlugin extends JavaPlugin implements Listener {
    private volatile boolean protect;
    private volatile boolean freeze;
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
        // Plugin messages: what comes on ferraloom:test goes back as "pong <it>".
        getServer().getMessenger().registerOutgoingPluginChannel(this, "ferraloom:test");
        getServer().getMessenger().registerIncomingPluginChannel(this, "ferraloom:test", (channel, player, data) -> {
            String text = new String(data, StandardCharsets.UTF_8);
            getLogger().info("message: " + channel + " " + text + " from " + player.getName());
            player.sendPluginMessage(this, channel, ("pong " + text).getBytes(StandardCharsets.UTF_8));
        });
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

    @EventHandler
    public void onPreLogin(AsyncPlayerPreLoginEvent event) {
        getLogger().info("event: prelogin " + event.getName() + " primary=" + Bukkit.isPrimaryThread());
        if (event.getName().equals("Refused")) {
            event.disallow(AsyncPlayerPreLoginEvent.Result.KICK_OTHER, "FerraloomTest refuses Refused");
        }
    }

    @EventHandler
    public void onLogin(PlayerLoginEvent event) {
        getLogger().info("event: login " + event.getPlayer().getName() + " result=" + event.getResult() + " primary="
                + Bukkit.isPrimaryThread());
    }

    @EventHandler
    public void onKick(PlayerKickEvent event) {
        getLogger().info("event: kick " + event.getPlayer().getName() + " " + event.getReason());
    }

    @EventHandler
    public void onMove(PlayerMoveEvent event) {
        if (freeze) {
            event.setCancelled(true);
            Location to = event.getTo();
            getLogger().info(String.format(Locale.ROOT, "event: move to %.1f %.1f %.1f cancelled", to.getX(), to.getY(),
                    to.getZ()));
        }
    }

    @EventHandler
    public void onPreprocess(PlayerCommandPreprocessEvent event) {
        if (event.getMessage().equals("/hello")) {
            event.setMessage("/ftest ping");
        } else if (event.getMessage().startsWith("/blocked")) {
            event.setCancelled(true);
            getLogger().info("event: preprocess cancelled " + event.getMessage());
        }
    }

    @EventHandler
    public void onInteract(PlayerInteractEvent event) {
        String block = event.hasBlock() ? event.getClickedBlock().getX() + " " + event.getClickedBlock().getY() + " "
                + event.getClickedBlock().getZ() : "-";
        getLogger().info("event: interact " + event.getAction() + " " + block + " " + event.getBlockFace() + " "
                + event.getHand() + " material=" + event.getMaterial());
    }

    @EventHandler
    public void onGameMode(PlayerGameModeChangeEvent event) {
        getLogger().info("event: gamemode " + event.getPlayer().getName() + " " + event.getNewGameMode());
    }

    @EventHandler
    public void onSneak(PlayerToggleSneakEvent event) {
        getLogger().info("event: sneak " + event.getPlayer().getName() + " " + event.isSneaking());
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
            case "stub" -> sender.sendMessage("shoulder " + ((Player) sender).getShoulderEntityLeft());
            case "state" -> {
                Player p = (Player) sender;
                p.setMetadata("ftest", new FixedMetadataValue(this, 42));
                sender.sendMessage(String.format(Locale.ROOT,
                        "state health=%.1f food=%d mode=%s level=%d flying=%s sneaking=%s locale=%s address=%s "
                                + "played=%s online=%s meta=%d",
                        p.getHealth(), p.getFoodLevel(), p.getGameMode(), p.getLevel(), p.isFlying(), p.isSneaking(),
                        p.getLocale(), p.getAddress().getAddress().getHostAddress(), p.hasPlayedBefore(),
                        p.getPlayer() == p, p.getMetadata("ftest").get(0).asInt()));
            }
            case "level" -> {
                ((Player) sender).setLevel(Integer.parseInt(args[1]));
                sender.sendMessage("level set");
            }
            case "mode" -> ((Player) sender).setGameMode(GameMode.valueOf(args[1].toUpperCase(Locale.ROOT)));
            case "show" -> {
                Player p = (Player) sender;
                p.sendTitle("Hello", "from FerraloomTest", 5, 40, 5);
                p.spigot().sendMessage(ChatMessageType.ACTION_BAR, new TextComponent("bar from FerraloomTest"));
                p.setPlayerListHeaderFooter("FerraloomTest header", "FerraloomTest footer");
                p.playSound(p.getLocation(), "minecraft:block.note_block.pling", 1f, 1f);
            }
            case "freeze" -> {
                freeze = args.length > 1 && args[1].equals("on");
                sender.sendMessage("freeze " + freeze);
            }
            case "run" -> ((Player) sender).performCommand("list");
            case "kickme" -> ((Player) sender).kickPlayer("FerraloomTest kicked you");
            default -> {
                return false;
            }
        }
        getLogger().info("command: " + sender.getName() + " ran " + label + " " + String.join(" ", args));
        return true;
    }
}
