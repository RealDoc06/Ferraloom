package dev.ferraloom.bukkit;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.util.Properties;

/**
 * server.properties as the Server getters read it (CraftServer reads the
 * dedicated server's parsed properties): read again when the file changes,
 * since the game stores it as commands change it ({@code /defaultgamemode},
 * {@code /whitelist on}). A missing or unreadable key is vanilla's default.
 */
final class FerraloomProperties {
    private final File file;
    private long readAt = Long.MIN_VALUE;
    private Properties values = new Properties();

    FerraloomProperties(File directory) {
        this.file = new File(directory, "server.properties");
    }

    private synchronized Properties values() {
        long modified = file.lastModified();
        if (modified != readAt) {
            Properties p = new Properties();
            try (InputStream in = Files.newInputStream(file.toPath())) {
                p.load(in);
            } catch (IOException e) {
                // Vanilla's defaults stand.
            }
            values = p;
            readAt = modified;
        }
        return values;
    }

    String string(String key, String fallback) {
        return values().getProperty(key, fallback);
    }

    int integer(String key, int fallback) {
        try {
            return Integer.parseInt(values().getProperty(key, "").trim());
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    boolean bool(String key, boolean fallback) {
        String v = values().getProperty(key);
        return v == null ? fallback : Boolean.parseBoolean(v.trim());
    }
}
