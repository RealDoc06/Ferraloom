package dev.ferraloom.nmsplugin;

import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * A multi-version plugin's shape: it probes for server internals by name and
 * handles their absence (so it loads and stays enabled), and its adapter
 * ({@link Adapter}) names one directly, which {@code /fnms} reaches: Ferraloom
 * then disables it with one line.
 */
public final class NmsPlugin extends JavaPlugin {
    @Override
    public void onEnable() {
        try {
            Class.forName("net.minecraft.server.MinecraftServer");
            getLogger().info("probe: internals found");
        } catch (ClassNotFoundException e) {
            getLogger().info("probe: no internals, enabled");
        }
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        sender.sendMessage("nms " + Adapter.server());
        return true;
    }
}
