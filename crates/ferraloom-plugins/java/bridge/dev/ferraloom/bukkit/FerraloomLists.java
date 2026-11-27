package dev.ferraloom.bukkit;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/**
 * The server's player lists and player files, for {@code Server},
 * {@code OfflinePlayer} and {@code Player}: operators, the whitelist, bans,
 * the user cache, and when a player played. Every method works on any
 * thread, as Bukkit allows (ServerNative's list natives lock the game's
 * lists; files are read directly).
 */
final class FerraloomLists {
    static final int OPS = 0;
    static final int WHITELIST = 1;
    static final int BANNED = 2;

    private FerraloomLists() {
    }

    /** A player the lists name: UUID and name (the name may be empty when unknown). */
    record Profile(UUID id, String name) {
    }

    static boolean isOp(UUID id) {
        return ServerNative.isListed(OPS, id.getMostSignificantBits(), id.getLeastSignificantBits());
    }

    /** PlayerList.op and deop: saved at once; the player's level and commands follow at the next tick. */
    static void setOp(UUID id, String name, boolean op) {
        ServerNative.setListed(OPS, id.getMostSignificantBits(), id.getLeastSignificantBits(), name, op);
    }

    static boolean isWhitelisted(UUID id) {
        return ServerNative.isListed(WHITELIST, id.getMostSignificantBits(), id.getLeastSignificantBits());
    }

    static void setWhitelisted(UUID id, String name, boolean on) {
        ServerNative.setListed(WHITELIST, id.getMostSignificantBits(), id.getLeastSignificantBits(), name, on);
    }

    static boolean isBanned(UUID id) {
        return ServerNative.isListed(BANNED, id.getMostSignificantBits(), id.getLeastSignificantBits());
    }

    static List<Profile> listed(int list) {
        String[] flat = ServerNative.listed(list);
        List<Profile> out = new ArrayList<>(flat.length / 2);
        for (int i = 0; i + 1 < flat.length; i += 2) {
            out.add(new Profile(UUID.fromString(flat[i]), flat[i + 1]));
        }
        return out;
    }

    /** The user cache's name for a UUID, or null. */
    static String cachedName(UUID id) {
        return ServerNative.profileById(id.getMostSignificantBits(), id.getLeastSignificantBits());
    }

    /**
     * CraftServer.getOfflinePlayer(String)'s profile: in online mode the user
     * cache's, else the profile service's (blocking, as CraftBukkit's);
     * otherwise, or when neither knows the name, the offline player's.
     */
    static Profile byName(String name, boolean onlineMode) {
        if (onlineMode) {
            String[] found = ServerNative.profileByName(name);
            if (found != null) {
                return new Profile(UUID.fromString(found[0]), found[1]);
            }
        }
        return new Profile(offlineId(name), name);
    }

    /** UUIDUtil.createOfflinePlayerUUID. */
    static UUID offlineId(String name) {
        return UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(StandardCharsets.UTF_8));
    }

    // Player files: <world>/players/data/<uuid>.dat, as the server saves them.

    static File dataFile(FerraloomServer server, UUID id) {
        return new File(dataFolder(server), id + ".dat");
    }

    static File dataFolder(FerraloomServer server) {
        return new File(new File(new File(server.directory, server.worlds.get(0).name), "players"), "data");
    }

    /** CraftOfflinePlayer.hasPlayedBefore: the player has a file. */
    static boolean hasPlayedBefore(FerraloomServer server, UUID id) {
        return dataFile(server, id).isFile();
    }

    /**
     * CraftOfflinePlayer.getFirstPlayed and getLastPlayed without CraftBukkit's
     * own "bukkit" tag in the file (Ferraloom writes none): the file's
     * modification time, 0 for a player who never played.
     */
    static long played(FerraloomServer server, UUID id) {
        return dataFile(server, id).lastModified();
    }

    /** WorldNBTStorage.getSeenPlayers: every UUID with a player file. */
    static List<UUID> seen(FerraloomServer server) {
        List<UUID> out = new ArrayList<>();
        File[] files = dataFolder(server).listFiles((dir, n) -> n.endsWith(".dat"));
        if (files != null) {
            for (File f : files) {
                try {
                    out.add(UUID.fromString(f.getName().substring(0, f.getName().length() - 4)));
                } catch (IllegalArgumentException ignored) {
                    // Not a player's file.
                }
            }
        }
        return out;
    }
}
