package dev.ferraloom.bukkit;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.bukkit.command.Command;
import org.bukkit.event.server.TabCompleteEvent;

/**
 * The server area's entry points (src/server.rs): plugin commands'
 * completions and which of them a player may see. Each is a wall, as
 * {@link Bridge}'s are: what a plugin throws is logged and the default answered.
 */
public final class ServerEvents {
    private static final String[] NONE = new String[0];

    private ServerEvents() {
    }

    /**
     * CraftServer.tabComplete for a plugin command line (no slash): the
     * command's TabCompleter (or its labels and player names, Bukkit's
     * defaults), then TabCompleteEvent. Completions are for the last word.
     */
    public static String[] tabComplete(long most, long least, String line) {
        try {
            FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
            if (player == null) {
                return NONE;
            }
            List<String> offers = Bridge.server.commandMap.tabComplete(player, line);
            TabCompleteEvent event = new TabCompleteEvent(player, "/" + line, offers == null ? new ArrayList<>() : offers);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled() ? NONE : event.getCompletions().toArray(NONE);
        } catch (Throwable ex) {
            Bridge.log(ex, "completing a command");
            return NONE;
        }
    }

    /** The plugin command labels a player may not use (Command.testPermissionSilent): left out of their command tree. */
    public static String[] hiddenCommands(long most, long least) {
        try {
            FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
            if (player == null) {
                return NONE;
            }
            List<String> hidden = new ArrayList<>();
            for (Map.Entry<String, Command> e : Bridge.server.commandMap.known().entrySet()) {
                if (!e.getValue().testPermissionSilent(player)) {
                    hidden.add(e.getKey());
                }
            }
            return hidden.toArray(NONE);
        } catch (Throwable ex) {
            Bridge.log(ex, "filtering the command tree");
            return NONE;
        }
    }
}
