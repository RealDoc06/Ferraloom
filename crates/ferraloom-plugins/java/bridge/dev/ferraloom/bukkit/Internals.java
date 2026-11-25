package dev.ferraloom.bukkit;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Enumeration;
import java.util.LinkedHashSet;
import java.util.Queue;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;
import java.util.logging.LogRecord;
import org.bukkit.plugin.Plugin;

/**
 * Plugins and Minecraft server internals ({@code net.minecraft},
 * {@code org.bukkit.craftbukkit}), which Ferraloom does not provide.
 * Multi-version plugins ship adapters for them that they pick by version
 * string and may never run, so such a plugin loads with one warning
 * ({@link #find}); if it does reach a missing internal class and lets the
 * error escape to Bukkit, the bridge logs one line instead of the trace and
 * disables that plugin at the next tick ({@link #reached}, {@link #disableReached}).
 * A plugin probing internals and handling the failure itself is left alone.
 */
final class Internals {
    private static final byte[][] PREFIXES = {
        "net/minecraft/".getBytes(StandardCharsets.US_ASCII),
        "org/bukkit/craftbukkit/".getBytes(StandardCharsets.US_ASCII),
    };
    /** How many internal class names the load warning lists. */
    private static final int NAMED = 3;

    /** Plugins that reached an internal, to disable on the server thread. */
    private static final Queue<Plugin> TO_DISABLE = new ConcurrentLinkedQueue<>();
    private static final Set<String> REACHED = ConcurrentHashMap.newKeySet();

    private Internals() {
    }

    /** What a jar names of server internals: a few class names, the first class naming one, and how many do. */
    record Found(Set<String> names, String from, int classes) {
        /** {@code net.minecraft.server.MinecraftServer and net.minecraft.world.level.Level (from com.example.v1_21.Adapter and 3 more classes)}. */
        String describe() {
            String more = classes > 1 ? " and " + (classes - 1) + " more class" + (classes > 2 ? "es" : "") : "";
            return String.join(", ", names) + " (from " + from + more + ")";
        }
    }

    /** The jar's references to server internals, or null when it has none. */
    static Found find(File jar) {
        Set<String> names = new LinkedHashSet<>();
        String from = null;
        int classes = 0;
        try (JarFile file = new JarFile(jar)) {
            Enumeration<JarEntry> entries = file.entries();
            while (entries.hasMoreElements()) {
                JarEntry entry = entries.nextElement();
                if (!entry.getName().endsWith(".class") || entry.getName().startsWith("META-INF/")) {
                    continue;
                }
                byte[] bytes;
                try (InputStream in = file.getInputStream(entry)) {
                    bytes = in.readAllBytes();
                }
                String found = scan(bytes);
                if (found != null) {
                    classes++;
                    if (from == null) {
                        String name = entry.getName().replace('/', '.');
                        from = name.substring(0, name.length() - ".class".length());
                    }
                    if (names.size() < NAMED) {
                        names.add(found);
                    }
                }
            }
        } catch (IOException e) {
            // Not a readable jar: the plugin loader reports it.
        }
        return from == null ? null : new Found(names, from, classes);
    }

    /** The internal class name in a class file's bytes, dotted, or null. */
    static String scan(byte[] bytes) {
        for (byte[] prefix : PREFIXES) {
            int at = indexOf(bytes, prefix);
            if (at >= 0) {
                int end = at;
                while (end < bytes.length && name(bytes[end])) {
                    end++;
                }
                return new String(bytes, at, end - at, StandardCharsets.US_ASCII).replace('/', '.');
            }
        }
        return null;
    }

    /**
     * Called by the log handler for a record with a throwable: true when the
     * throwable is a missing server internal, which is then logged as one line
     * (and its plugin queued for disabling) instead of the record. Costs
     * nothing on the paths that log no throwable.
     */
    static boolean reached(LogRecord record) {
        Throwable missing = missingInternal(record.getThrown());
        if (missing == null || Bridge.server == null) {
            return false;
        }
        String internal = internalName(missing);
        Plugin plugin = owner(missing);
        if (plugin == null) {
            String what = record.getMessage() == null ? "A plugin failed" : record.getMessage();
            Bridge.server.logger.severe(what + ": it reached " + internal
                    + ", a Minecraft server internal (NMS or CraftBukkit) that Ferraloom does not provide");
            return true;
        }
        if (REACHED.add(plugin.getName())) {
            Bridge.server.logger.severe("Disabling " + plugin.getDescription().getFullName() + ": it reached " + internal
                    + " (from " + frame(missing) + "), a Minecraft server internal (NMS or CraftBukkit) that Ferraloom does not provide");
            TO_DISABLE.add(plugin);
        }
        return true;
    }

    /** On the server thread, at the start of a tick: disables the plugins {@link #reached} found. */
    static void disableReached() {
        for (Plugin plugin; (plugin = TO_DISABLE.poll()) != null; ) {
            Bridge.server.pluginManager.disablePlugin(plugin);
        }
    }

    /** The NoClassDefFoundError (or the ClassNotFoundException) for an internal class in the cause chain, or null. */
    private static Throwable missingInternal(Throwable t) {
        for (int depth = 0; t != null && depth < 16; t = t.getCause(), depth++) {
            if ((t instanceof NoClassDefFoundError || t instanceof ClassNotFoundException) && internalName(t) != null) {
                return t;
            }
        }
        return null;
    }

    private static String internalName(Throwable t) {
        String message = t.getMessage();
        if (message == null) {
            return null;
        }
        String dotted = message.replace('/', '.');
        int at = dotted.indexOf("net.minecraft.");
        if (at < 0) {
            at = dotted.indexOf("org.bukkit.craftbukkit");
        }
        if (at < 0) {
            return null;
        }
        int end = at;
        while (end < dotted.length() && (Character.isJavaIdentifierPart(dotted.charAt(end)) || dotted.charAt(end) == '.')) {
            end++;
        }
        return dotted.substring(at, end);
    }

    /** The plugin whose class the error was thrown in. */
    private static Plugin owner(Throwable t) {
        return owner(t.getStackTrace());
    }

    /** The plugin of the first frame a plugin's class loader defined, or null. */
    static Plugin owner(StackTraceElement[] stack) {
        for (StackTraceElement frame : stack) {
            for (Plugin plugin : Bridge.server.pluginManager.getPlugins()) {
                ClassLoader loader = plugin.getClass().getClassLoader();
                try {
                    if (Class.forName(frame.getClassName(), false, loader).getClassLoader() == loader) {
                        return plugin;
                    }
                } catch (ClassNotFoundException | LinkageError e) {
                    // Not this plugin's class.
                }
            }
        }
        return null;
    }

    private static String frame(Throwable t) {
        StackTraceElement[] trace = t.getStackTrace();
        return trace.length == 0 ? "?" : trace[0].getClassName() + "." + trace[0].getMethodName();
    }

    private static boolean name(byte b) {
        return b == '/' || b == '_' || b == '$' || (b >= '0' && b <= '9') || (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z');
    }

    private static int indexOf(byte[] haystack, byte[] needle) {
        outer:
        for (int i = 0; i + needle.length <= haystack.length; i++) {
            for (int j = 0; j < needle.length; j++) {
                if (haystack[i + j] != needle[j]) {
                    continue outer;
                }
            }
            return i;
        }
        return -1;
    }
}
