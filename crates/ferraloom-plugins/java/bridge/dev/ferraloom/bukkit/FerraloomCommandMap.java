package dev.ferraloom.bukkit;

import org.bukkit.Server;
import org.bukkit.command.Command;
import org.bukkit.command.SimpleCommandMap;

/**
 * Bukkit's command map, counting its changes so the server resends its
 * command tree. Bukkit's /reload is left out (the vanilla /reload stays, and
 * reloading plugins in place is not supported), and so is /timings, which
 * needs Spigot's timings.
 */
final class FerraloomCommandMap extends SimpleCommandMap {
    /** Bumped by every registration; no initializer, as the super constructor registers first. */
    private int generation;

    FerraloomCommandMap(Server server) {
        super(server);
        for (String label : new String[] {"reload", "bukkit:reload", "rl", "bukkit:rl", "timings", "bukkit:timings"}) {
            knownCommands.remove(label);
        }
    }

    @Override
    public boolean register(String label, String fallbackPrefix, Command command) {
        generation++;
        return super.register(label, fallbackPrefix, command);
    }

    @Override
    public synchronized void clearCommands() {
        generation++;
        super.clearCommands();
    }

    /** A value that changes whenever the labels may have: the registrations and the label count. */
    long version() {
        return ((long) generation << 32) | knownCommands.size();
    }

    /** Label to command, fallback-prefixed labels and aliases included. */
    java.util.Map<String, Command> known() {
        return knownCommands;
    }

    /** Every label the map answers to, fallback-prefixed ones and aliases included. */
    String[] labels() {
        return knownCommands.keySet().toArray(new String[0]);
    }
}
