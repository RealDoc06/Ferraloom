package dev.ferraloom.bukkit;

import java.util.concurrent.TimeUnit;
import org.bukkit.plugin.Plugin;

/**
 * Spigot's WatchdogThread, as a warning only: when the server thread has
 * been inside the same call into Java (plugin code, or the bridge running
 * it) for 10 seconds without a new tick starting, its Java stack is logged,
 * naming the plugin whose code is deepest in it, then again every 5 seconds
 * while it lasts. Looked at once a second, and only when no tick has started
 * since the last look: a stall in the server's own (Rust) code has no Java
 * stack and is not reported, nor is a plugin that is merely slow (ticks keep
 * starting), nor startup. Exists only with plugins; costs a volatile write
 * per tick and a wake-up per second that reads it.
 */
final class Watchdog extends Thread {
    private static final long WARN_AFTER = TimeUnit.SECONDS.toNanos(10);
    private static final long REPEAT = TimeUnit.SECONDS.toNanos(5);

    /** Set as each tick starts (and as shutdown does). */
    private static volatile long tickNumber;
    /** From the first tick on: startup (onLoad, onEnable) is not watched, as Spigot's watchdog does not. */
    private static volatile boolean armed;

    private final Thread server;

    private Watchdog(Thread server) {
        super("Ferraloom Watchdog");
        this.server = server;
        setDaemon(true);
    }

    static void start(Thread server) {
        new Watchdog(server).start();
    }

    /** A tick starts (Bridge.tick), or shutdown does. */
    static void ticked(long tick) {
        tickNumber = tick;
        armed = true;
    }

    @Override
    public void run() {
        StackTraceElement entry = null;
        long since = 0;
        long tick = 0;
        long reported = 0;
        while (true) {
            try {
                Thread.sleep(1000);
            } catch (InterruptedException e) {
                return;
            }
            long t = tickNumber;
            if (!armed || t != tick) {
                // Ticks are starting: nothing to look at.
                tick = t;
                entry = null;
                continue;
            }
            StackTraceElement[] stack = server.getStackTrace();
            long now = System.nanoTime();
            StackTraceElement bottom = stack.length == 0 ? null : stack[stack.length - 1];
            if (bottom == null || !bottom.equals(entry)) {
                // Out of Java, or in another call: start over.
                entry = bottom;
                since = now;
                reported = 0;
                continue;
            }
            if (now - since >= WARN_AFTER && (reported == 0 || now - reported >= REPEAT)) {
                reported = now;
                try {
                    report(stack, now - since);
                } catch (RuntimeException e) {
                    // The watchdog must outlive whatever a report trips over.
                    Bridge.server.logger.warning("The watchdog could not report a stall: " + e);
                }
            }
        }
    }

    private static void report(StackTraceElement[] stack, long stalled) {
        Plugin plugin = Internals.owner(stack);
        StringBuilder s = new StringBuilder("The server thread has been stuck in ")
                .append(plugin == null ? "plugin code" : plugin.getDescription().getFullName()).append(" for ")
                .append(TimeUnit.NANOSECONDS.toSeconds(stalled)).append(" s (tick ").append(tickNumber).append("):");
        for (StackTraceElement frame : stack) {
            s.append("\n\tat ").append(frame);
        }
        Bridge.server.logger.warning(s.toString());
    }
}
