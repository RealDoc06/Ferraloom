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
}
