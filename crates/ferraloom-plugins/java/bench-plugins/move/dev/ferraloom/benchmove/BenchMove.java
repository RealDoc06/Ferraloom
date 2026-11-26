package dev.ferraloom.benchmove;

import org.bukkit.Location;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Listens to PlayerMoveEvent, the busiest event a server fires, and does what a region plugin does with it:
 * compares the two block positions. Prints its counts at stop, so a run shows the listener was called
 * (tools/ferraloom-bench/README.md).
 */
public final class BenchMove extends JavaPlugin implements Listener {
    private long moves;
    private long blocks;

    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);
    }

    @EventHandler
    public void onMove(PlayerMoveEvent event) {
        moves++;
        Location from = event.getFrom();
        Location to = event.getTo();
        if (to != null && (from.getBlockX() != to.getBlockX() || from.getBlockY() != to.getBlockY()
                || from.getBlockZ() != to.getBlockZ())) {
            blocks++;
        }
    }

    @Override
    public void onDisable() {
        getLogger().info("moves " + moves + ", block changes " + blocks);
    }
}
