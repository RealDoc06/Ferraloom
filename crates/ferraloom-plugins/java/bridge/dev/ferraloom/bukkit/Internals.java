package dev.ferraloom.bukkit;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Enumeration;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;

/**
 * Finds a plugin's use of Minecraft server internals before it loads: a
 * class file whose constant pool names a {@code net/minecraft/} or
 * {@code org/bukkit/craftbukkit/} class. Ferraloom has neither, so such a
 * plugin fails at load with one line instead of a NoClassDefFoundError later.
 */
final class Internals {
    private static final byte[][] PREFIXES = {
        "net/minecraft/".getBytes(StandardCharsets.US_ASCII),
        "org/bukkit/craftbukkit/".getBytes(StandardCharsets.US_ASCII),
    };

    private Internals() {
    }

    /** The first internal class the jar refers to, as {@code net.minecraft.server.MinecraftServer (from com.example.Foo)}, or null. */
    static String find(File jar) {
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
                    String from = entry.getName().replace('/', '.');
                    return found + " (from " + from.substring(0, from.length() - ".class".length()) + ")";
                }
            }
        } catch (IOException e) {
            // Not a readable jar: the plugin loader reports it.
        }
        return null;
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
