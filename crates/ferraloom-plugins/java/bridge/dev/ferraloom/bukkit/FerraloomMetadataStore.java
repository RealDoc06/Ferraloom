package dev.ferraloom.bukkit;

import java.util.List;
import java.util.UUID;
import org.bukkit.World;
import org.bukkit.metadata.MetadataStoreBase;
import org.bukkit.metadata.MetadataValue;
import org.bukkit.plugin.Plugin;

/**
 * The one store behind every {@code Metadatable} the bridge implements (players, entities, blocks), as
 * CraftServer's PlayerMetadataStore, EntityMetadataStore and BlockMetadataStore are. A subject is named by a
 * string: {@link #player}, {@link #entity} or {@link #block}. An implementation's four Metadatable methods are
 * one line each:
 *
 * <pre>
 * public void setMetadata(String key, MetadataValue value) { FerraloomMetadataStore.STORE.setMetadata(subject, key, value); }
 * public List&lt;MetadataValue&gt; getMetadata(String key) { return FerraloomMetadataStore.STORE.getMetadata(subject, key); }
 * public boolean hasMetadata(String key) { return FerraloomMetadataStore.STORE.hasMetadata(subject, key); }
 * public void removeMetadata(String key, Plugin plugin) { FerraloomMetadataStore.STORE.removeMetadata(subject, key, plugin); }
 * </pre>
 *
 * Thread-safe (MetadataStoreBase's methods are synchronized). Values outlive the object that set them, as in
 * CraftBukkit: a player who reconnects keeps their metadata.
 */
final class FerraloomMetadataStore extends MetadataStoreBase<String> {
    static final FerraloomMetadataStore STORE = new FerraloomMetadataStore();

    private FerraloomMetadataStore() {
    }

    /** PlayerMetadataStore's key: the name, lower case (an OfflinePlayer's metadata is a player's). */
    static String player(String name) {
        return "player:" + name.toLowerCase(java.util.Locale.ROOT);
    }

    /** EntityMetadataStore's key: the UUID. */
    static String entity(UUID id) {
        return "entity:" + id;
    }

    /** BlockMetadataStore's key: the world and the position. */
    static String block(World world, int x, int y, int z) {
        return "block:" + world.getUID() + ":" + x + ":" + y + ":" + z;
    }

    @Override
    protected String disambiguate(String subject, String key) {
        return subject + ":" + key;
    }

    // Shorthands for the Metadatable methods.

    static void set(String subject, String key, MetadataValue value) {
        STORE.setMetadata(subject, key, value);
    }

    static List<MetadataValue> get(String subject, String key) {
        return STORE.getMetadata(subject, key);
    }

    static boolean has(String subject, String key) {
        return STORE.hasMetadata(subject, key);
    }

    static void remove(String subject, String key, Plugin plugin) {
        STORE.removeMetadata(subject, key, plugin);
    }
}
