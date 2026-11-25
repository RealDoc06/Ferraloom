package dev.ferraloom.testplugin;

import java.util.Locale;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Animals;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Monster;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.entity.CreatureSpawnEvent;
import org.bukkit.event.entity.EntityDamageByEntityEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.EntityDeathEvent;
import org.bukkit.event.player.PlayerInteractEntityEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** The entities area's checks (tools/plugins_e2e.py): events on the last cow summoned, and /fentity. */
final class EntityChecks implements Listener, CommandExecutor {
    private final JavaPlugin plugin;
    /** The last cow a command spawned: the entity the checks act on. */
    private LivingEntity cow;
    private boolean noPigs;
    private boolean shield;
    private boolean quadruple;

    EntityChecks(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    private void log(String line) {
        plugin.getLogger().info(line);
    }

    @EventHandler
    public void onSpawn(CreatureSpawnEvent event) {
        CreatureSpawnEvent.SpawnReason reason = event.getSpawnReason();
        if (reason == CreatureSpawnEvent.SpawnReason.NATURAL || reason == CreatureSpawnEvent.SpawnReason.CHUNK_GEN) {
            return;
        }
        LivingEntity e = event.getEntity();
        if (e.getType() == EntityType.PIG && noPigs) {
            event.setCancelled(true);
        }
        if (e.getType() == EntityType.COW) {
            cow = e;
            e.setAI(false);
        }
        log("event: spawn " + e.getType() + " id=" + e.getEntityId() + " reason=" + reason + " cancelled="
                + event.isCancelled());
    }

    @EventHandler
    public void onDamage(EntityDamageEvent event) {
        if (!event.getEntity().equals(cow)) {
            return;
        }
        String by = event instanceof EntityDamageByEntityEvent e ? e.getDamager().getName() : "none";
        boolean player = event instanceof EntityDamageByEntityEvent e && e.getDamager() instanceof Player;
        if (player && shield) {
            event.setCancelled(true);
        } else if (player && quadruple) {
            event.setDamage(event.getDamage() * 4);
        }
        log(String.format(Locale.ROOT, "event: damage %s by %s cause=%s final=%.1f cancelled=%s",
                event.getEntityType(), by, event.getCause(), event.getFinalDamage(), event.isCancelled()));
    }

    @EventHandler
    public void onDeath(EntityDeathEvent event) {
        if (event.getEntity().equals(cow)) {
            Player killer = event.getEntity().getKiller();
            log("event: death " + event.getEntityType() + " exp=" + event.getDroppedExp() + " killer="
                    + (killer == null ? "none" : killer.getName()));
        }
    }

    @EventHandler
    public void onInteract(PlayerInteractEntityEvent event) {
        event.setCancelled(true);
        log("event: interact " + event.getRightClicked().getType() + " by " + event.getPlayer().getName() + " hand="
                + event.getHand() + " cancelled=true");
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        String sub = args.length == 0 ? "" : args[0].toLowerCase(Locale.ROOT);
        boolean on = args.length > 1 && args[1].equals("on");
        switch (sub) {
            case "pigs" -> {
                noPigs = !on;
                sender.sendMessage("pigs " + on);
            }
            case "shield" -> {
                shield = on;
                sender.sendMessage("shield " + on);
            }
            case "quadruple" -> {
                quadruple = on;
                sender.sendMessage("quadruple " + on);
            }
            case "name" -> {
                cow.setCustomName(args.length > 1 ? args[1] : null);
                sender.sendMessage("named " + cow.getCustomName());
            }
            case "kill" -> cow.setHealth(0);
            case "remove" -> cow.remove();
            case "mob" -> {
                Entity e = cow;
                sender.sendMessage(String.format(Locale.ROOT,
                        "mob %s health=%.1f max=%.1f name=%s ai=%s valid=%s animals=%s monster=%s", e.getType(),
                        cow.getHealth(), cow.getMaxHealth(), e.getCustomName(), cow.hasAI(), e.isValid(),
                        e instanceof Animals, e instanceof Monster));
            }
            default -> {
                return false;
            }
        }
        return true;
    }
}
