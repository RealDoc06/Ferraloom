package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubProfileBanList;
import java.time.Duration;
import java.time.Instant;
import java.util.Date;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.UUID;
import org.bukkit.BanEntry;
import org.bukkit.profile.PlayerProfile;

/** CraftProfileBanList: banned-players.json, by player (a String target is a UUID or a name). */
@SuppressWarnings({"rawtypes", "unchecked"})
final class FerraloomProfileBans extends StubProfileBanList {
    private final FerraloomServer server;

    FerraloomProfileBans(FerraloomServer server) {
        this.server = server;
    }

    /** CraftProfileBanList.getProfile: a UUID (its cached name), else a name as getOfflinePlayer resolves it. */
    private PlayerProfile profile(String target) {
        if (target == null || target.isBlank()) {
            throw new IllegalArgumentException("Target cannot be null or blank");
        }
        try {
            UUID id = UUID.fromString(target);
            return new FerraloomProfile(id, FerraloomLists.cachedName(id));
        } catch (IllegalArgumentException notUuid) {
            return server.getOfflinePlayer(target).getPlayerProfile();
        }
    }

    private static UUID require(PlayerProfile target) {
        if (target == null || target.getUniqueId() == null) {
            throw new IllegalArgumentException("Target's UUID cannot be null");
        }
        return target.getUniqueId();
    }

    private java.util.List<FerraloomBanEntry<PlayerProfile>> entries() {
        return FerraloomBanEntry.read(false, (name, id) -> new FerraloomProfile(id, name));
    }

    @Override
    public BanEntry<PlayerProfile> getBanEntry(String target) {
        return getBanEntry(profile(target));
    }

    @Override
    public BanEntry<PlayerProfile> getBanEntry(PlayerProfile target) {
        UUID id = require(target);
        for (FerraloomBanEntry<PlayerProfile> e : entries()) {
            if (id.equals(e.id())) {
                return e;
            }
        }
        return null;
    }

    @Override
    public BanEntry<PlayerProfile> addBan(String target, String reason, Date expires, String source) {
        return addBan(profile(target), reason, expires, source);
    }

    @Override
    public BanEntry<PlayerProfile> addBan(PlayerProfile target, String reason, Date expires, String source) {
        FerraloomBanEntry.write(target.getName() == null ? "" : target.getName(), require(target), null, source, expires, reason);
        return getBanEntry(target);
    }

    @Override
    public BanEntry<PlayerProfile> addBan(PlayerProfile target, String reason, Instant expires, String source) {
        return addBan(target, reason, FerraloomBanEntry.expiry(expires), source);
    }

    @Override
    public BanEntry<PlayerProfile> addBan(PlayerProfile target, String reason, Duration duration, String source) {
        return addBan(target, reason, FerraloomBanEntry.expiry(duration), source);
    }

    @Override
    public Set<BanEntry> getBanEntries() {
        return new LinkedHashSet<>(entries());
    }

    @Override
    public Set<BanEntry<PlayerProfile>> getEntries() {
        return new LinkedHashSet<>(entries());
    }

    @Override
    public boolean isBanned(PlayerProfile target) {
        return FerraloomLists.isBanned(require(target));
    }

    @Override
    public boolean isBanned(String target) {
        return isBanned(profile(target));
    }

    @Override
    public void pardon(PlayerProfile target) {
        FerraloomBanEntry.pardon(target.getName(), require(target));
    }

    @Override
    public void pardon(String target) {
        pardon(profile(target));
    }
}
