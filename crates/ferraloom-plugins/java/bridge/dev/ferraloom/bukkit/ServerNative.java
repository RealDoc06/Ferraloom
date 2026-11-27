package dev.ferraloom.bukkit;

/**
 * The server area's calls into the Rust server (src/server.rs). Each area
 * has its own class of natives ({@link PlayerNative}, {@link WorldNative}),
 * registered with RegisterNatives at start; keep both sides' signatures in step.
 *
 * <p>Calls that read or change the game answer only on the server thread,
 * inside a call the server made into Java; elsewhere they throw
 * {@link IllegalStateException}. Messages, log lines and shutdown work from
 * any thread: off the server thread they are queued for the next tick.
 */
final class ServerNative {
    private ServerNative() {
    }

    /** A line in the server's log, as the thread {@code thread} at {@code level} (INFO, WARN, ERROR). */
    static native void log(String level, String thread, String message);

    /** A line on the server console, as a command's reply to it. */
    static native void consoleMessage(String message);

    /** Runs a vanilla command line as the console; false when the game has no such command. */
    static native boolean dispatchVanilla(String line);

    /** Stops the server after the current tick. */
    static native void shutdown();

    // The player lists (src/server/lists.rs), answered on any thread. Lists: 0 ops, 1 whitelist, 2 banned players.

    /** The user cache's {uuid, name} for a name, else the profile service's (blocking); null when neither knows it. */
    static native String[] profileByName(String name);

    /** The user cache's name for a UUID, or null; never a lookup. */
    static native String profileById(long most, long least);

    /** A list's players as {uuid, name, uuid, name...}. */
    static native String[] listed(int list);

    static native boolean isListed(int list, long most, long least);

    /** Adds to or removes from the operators (list 0) or the whitelist (1); saved at once. */
    static native void setListed(int list, long most, long least, String name, boolean on);

    /** {@code white-list} (0) or {@code enforce-whitelist} (1). */
    static native boolean whitelistFlag(int flag);

    /** Changes a whitelist flag and stores it in server.properties. */
    static native void setWhitelistFlag(int flag, boolean on);

    static native void reloadWhitelist();

    /** Six strings per ban: target, uuid (null for an address), created millis, expiry millis (null: never), source, reason (may be null). */
    static native String[] bans(boolean ip);

    /** Bans a player (a nonzero UUID, {@code target} their name) or an address; {@code created} and {@code expires} -1 for now and never. */
    static native void addBan(String target, long most, long least, long created, String source, long expires, String reason);

    /** Lifts the ban of a player (a nonzero UUID) or of an address. */
    static native void pardon(String target, long most, long least);

    /** Sends a player their command tree again at the next tick's start (they joined; Player.updateCommands). */
    static native void updateCommands(long most, long least);

    /**
     * A plugin boss bar's whole state (src/server/bossbars.rs), from any thread: flags bit 0 darken the sky,
     * 1 boss music, 2 fog, 3 visible; {@code players} their UUIDs' halves. A hidden bar or one with no players is shown to no one.
     */
    static native void bossBar(long most, long least, String title, float progress, int color, int style, int flags, long[] players);
}
