package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubOfflinePlayer;
import java.time.Duration;
import java.time.Instant;
import java.util.Date;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.UUID;
import org.bukkit.BanEntry;
import org.bukkit.entity.Player;
import org.bukkit.profile.PlayerProfile;

/**
 * CraftOfflinePlayer: a player by UUID, online or not, answered from the
 * server's lists and player files ({@link FerraloomLists}) on any thread.
 * Two are equal when their UUIDs are.
 */
final class FerraloomOfflinePlayer extends StubOfflinePlayer {
    private final FerraloomServer server;
    private final UUID id;
    /** The name it was asked by, or "" (then the online player's or the user cache's answers). */
    private final String knownName;

    FerraloomOfflinePlayer(FerraloomServer server, UUID id, String name) {
        this.server = server;
        this.id = id;
        this.knownName = name == null ? "" : name;
    }

    @Override
    public UUID getUniqueId() {
        return id;
    }

    /** CraftOfflinePlayer.getName: the online player's, the profile's, else the user cache's; null when unknown. */
    @Override
    public String getName() {
        Player online = getPlayer();
        if (online != null) {
            return online.getName();
        }
        return knownName.isEmpty() ? FerraloomLists.cachedName(id) : knownName;
    }

    @Override
    public boolean isOnline() {
        return getPlayer() != null;
    }

    @Override
    public Player getPlayer() {
        return server.getPlayer(id);
    }

    @Override
    public PlayerProfile getPlayerProfile() {
        return new FerraloomProfile(id, getName());
    }

    @Override
    public boolean isOp() {
        return FerraloomLists.isOp(id);
    }

    @Override
    public void setOp(boolean op) {
        FerraloomLists.setOp(id, getName(), op);
    }

    @Override
    public boolean isWhitelisted() {
        return FerraloomLists.isWhitelisted(id);
    }

    @Override
    public void setWhitelisted(boolean on) {
        FerraloomLists.setWhitelisted(id, getName(), on);
    }

    @Override
    public boolean isBanned() {
        return FerraloomLists.isBanned(id);
    }

    @Override
    public BanEntry<PlayerProfile> ban(String reason, Date expires, String source) {
        return server.profileBans.addBan(getPlayerProfile(), reason, expires, source);
    }

    @Override
    public BanEntry<PlayerProfile> ban(String reason, Instant expires, String source) {
        return server.profileBans.addBan(getPlayerProfile(), reason, expires, source);
    }

    @Override
    public BanEntry<PlayerProfile> ban(String reason, Duration duration, String source) {
        return server.profileBans.addBan(getPlayerProfile(), reason, duration, source);
    }

    @Override
    public boolean hasPlayedBefore() {
        return FerraloomLists.hasPlayedBefore(server, id);
    }

    @Override
    public long getFirstPlayed() {
        return FerraloomLists.played(server, id);
    }

    @Override
    public long getLastPlayed() {
        return FerraloomLists.played(server, id);
    }

    /** CraftOfflinePlayer.serialize, read back by Bukkit's ConfigurationSerialization through deserialize. */
    @Override
    public Map<String, Object> serialize() {
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("UUID", id.toString());
        return out;
    }

    /** ConfigurationSerialization's way back from {@link #serialize}. */
    public static FerraloomOfflinePlayer deserialize(Map<String, Object> args) {
        return (FerraloomOfflinePlayer) Bridge.server.getOfflinePlayer(UUID.fromString((String) args.get("UUID")));
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof FerraloomOfflinePlayer other && other.id.equals(id);
    }

    @Override
    public int hashCode() {
        return id.hashCode();
    }

    @Override
    public String toString() {
        return "FerraloomOfflinePlayer[UUID=" + id + "]";
    }
}
