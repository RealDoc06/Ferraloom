package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubUnsafeValues;
import org.bukkit.Bukkit;
import org.bukkit.plugin.InvalidPluginException;
import org.bukkit.plugin.PluginDescriptionFile;

/** The two UnsafeValues the plugin loader itself calls; the rest is stubbed. */
final class FerraloomUnsafe extends StubUnsafeValues {
    /** The Minecraft version this API is for: the {@code api-version} plugins may ask for, at most. */
    static final String MINECRAFT = "26.3";

    /** CraftMagicNumbers.checkSupported: a plugin asking for a newer API than this one does not load. */
    @Override
    public void checkSupported(PluginDescriptionFile pdf) throws InvalidPluginException {
        String wanted = pdf.getAPIVersion();
        if (wanted == null) {
            Bukkit.getLogger().warning("Legacy plugin " + pdf.getFullName()
                    + " does not specify an api-version; Ferraloom loads it without legacy material support");
            return;
        }
        if (compare(wanted, MINECRAFT) > 0) {
            throw new InvalidPluginException("Unsupported API version " + wanted);
        }
    }

    /** 26.3's world data version (horizon_core's), which plugins compare item and chunk data against. */
    @Override
    public int getDataVersion() {
        return 5023;
    }

    /** Classes load as they are: Ferraloom rewrites no plugin bytecode. */
    @Override
    public byte[] processClass(PluginDescriptionFile pdf, String path, byte[] clazz) {
        return clazz;
    }

    // Ferraloom: registries (blocks-worlds area, FerraloomRegistries): OldEnum.valueOf and Biome.CUSTOM.
    @Override public <B extends org.bukkit.Keyed> B get(org.bukkit.Registry<B> registry, org.bukkit.NamespacedKey key) { return key == null ? null : registry.get(key); }
    @Override public org.bukkit.block.Biome getCustomBiome() { return FerraloomRegistries.CUSTOM_BIOME; }

    /** Dotted version numbers, part by part ({@code 1.21} < {@code 26.3}); a part that is not a number counts as 0. */
    static int compare(String a, String b) {
        String[] x = a.split("\\.");
        String[] y = b.split("\\.");
        for (int i = 0; i < Math.max(x.length, y.length); i++) {
            int c = Integer.compare(part(x, i), part(y, i));
            if (c != 0) {
                return c;
            }
        }
        return 0;
    }

    private static int part(String[] parts, int i) {
        if (i >= parts.length) {
            return 0;
        }
        try {
            return Integer.parseInt(parts[i]);
        } catch (NumberFormatException e) {
            return 0;
        }
    }
}
