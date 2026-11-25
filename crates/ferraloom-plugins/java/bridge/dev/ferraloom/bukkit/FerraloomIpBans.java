package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubIpBanList;
import java.net.InetAddress;
import java.net.UnknownHostException;
import java.time.Duration;
import java.time.Instant;
import java.util.Date;
import java.util.LinkedHashSet;
import java.util.Set;
import org.bukkit.BanEntry;

/** CraftIpBanList: banned-ips.json, by address (a String target is the address as text). */
@SuppressWarnings({"rawtypes", "unchecked"})
final class FerraloomIpBans extends StubIpBanList {
    private static InetAddress address(String ip) {
        try {
            return InetAddress.getByName(ip);
        } catch (UnknownHostException e) {
            return null;
        }
    }

    private static String text(InetAddress address) {
        if (address == null) {
            throw new IllegalArgumentException("Address cannot be null");
        }
        return address.getHostAddress();
    }

    private static String text(String target) {
        if (target == null || target.isBlank()) {
            throw new IllegalArgumentException("Address cannot be null or blank");
        }
        return target;
    }

    private java.util.List<FerraloomBanEntry<InetAddress>> entries() {
        return FerraloomBanEntry.read(true, (ip, id) -> address(ip));
    }

    @Override
    public BanEntry<InetAddress> getBanEntry(String target) {
        String ip = text(target);
        for (FerraloomBanEntry<InetAddress> e : entries()) {
            if (e.getTarget().equals(ip)) {
                return e;
            }
        }
        return null;
    }

    @Override
    public BanEntry<InetAddress> getBanEntry(InetAddress target) {
        return getBanEntry(text(target));
    }

    @Override
    public BanEntry<InetAddress> addBan(String target, String reason, Date expires, String source) {
        FerraloomBanEntry.write(text(target), null, null, source, expires, reason);
        return getBanEntry(target);
    }

    @Override
    public BanEntry<InetAddress> addBan(InetAddress target, String reason, Date expires, String source) {
        return addBan(text(target), reason, expires, source);
    }

    @Override
    public BanEntry<InetAddress> addBan(InetAddress target, String reason, Instant expires, String source) {
        return addBan(text(target), reason, FerraloomBanEntry.expiry(expires), source);
    }

    @Override
    public BanEntry<InetAddress> addBan(InetAddress target, String reason, Duration duration, String source) {
        return addBan(text(target), reason, FerraloomBanEntry.expiry(duration), source);
    }

    @Override
    public Set<BanEntry> getBanEntries() {
        return new LinkedHashSet<>(entries());
    }

    @Override
    public Set<BanEntry<InetAddress>> getEntries() {
        return new LinkedHashSet<>(entries());
    }

    @Override
    public boolean isBanned(InetAddress target) {
        return getBanEntry(target) != null;
    }

    @Override
    public boolean isBanned(String target) {
        return getBanEntry(target) != null;
    }

    @Override
    public void pardon(InetAddress target) {
        pardon(text(target));
    }

    @Override
    public void pardon(String target) {
        FerraloomBanEntry.pardon(text(target), null);
    }
}
