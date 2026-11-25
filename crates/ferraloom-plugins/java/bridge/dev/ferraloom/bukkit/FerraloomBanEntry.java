package dev.ferraloom.bukkit;

import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;
import java.util.UUID;
import org.bukkit.BanEntry;

/**
 * CraftProfileBanEntry and CraftIpBanEntry: a ban as read, changed by its
 * setters and written back by {@link #save}. {@code id} is the banned
 * player's UUID, null for an address.
 */
final class FerraloomBanEntry<T> implements BanEntry<T> {
    private final String target;
    private final UUID id;
    private final T banTarget;
    private Date created;
    private String source;
    private Date expiration;
    private String reason;

    private FerraloomBanEntry(String target, UUID id, T banTarget, Date created, String source, Date expiration, String reason) {
        this.target = target;
        this.id = id;
        this.banTarget = banTarget;
        this.created = created;
        this.source = source;
        this.expiration = expiration;
        this.reason = reason;
    }

    /** One list's bans (ServerNative.bans), each target made a T by {@code make(name, uuid)}. */
    static <T> List<FerraloomBanEntry<T>> read(boolean ip, java.util.function.BiFunction<String, UUID, T> make) {
        String[] flat = ServerNative.bans(ip);
        List<FerraloomBanEntry<T>> out = new ArrayList<>(flat.length / 6);
        for (int i = 0; i + 5 < flat.length; i += 6) {
            UUID id = flat[i + 1] == null ? null : UUID.fromString(flat[i + 1]);
            Date expires = flat[i + 3] == null ? null : new Date(Long.parseLong(flat[i + 3]));
            out.add(new FerraloomBanEntry<>(flat[i], id, make.apply(flat[i], id), new Date(Long.parseLong(flat[i + 2])),
                    flat[i + 4], expires, flat[i + 5]));
        }
        return out;
    }

    /** CraftProfileBanList.addBan's blank-to-null: a blank source or reason takes vanilla's default. */
    static String blankToNull(String s) {
        return s == null || s.isBlank() ? null : s;
    }

    static Date expiry(Instant expires) {
        return expires == null ? null : Date.from(expires);
    }

    static Date expiry(Duration duration) {
        return duration == null ? null : Date.from(Instant.now().plus(duration));
    }

    /** Writes a ban: a player's when {@code id} is set, else an address's. */
    static void write(String target, UUID id, Date created, String source, Date expires, String reason) {
        ServerNative.addBan(target, id == null ? 0 : id.getMostSignificantBits(), id == null ? 0 : id.getLeastSignificantBits(),
                created == null ? -1 : created.getTime(), blankToNull(source), expires == null ? -1 : expires.getTime(),
                blankToNull(reason));
    }

    static void pardon(String target, UUID id) {
        ServerNative.pardon(target, id == null ? 0 : id.getMostSignificantBits(), id == null ? 0 : id.getLeastSignificantBits());
    }

    UUID id() {
        return id;
    }

    @Override
    public String getTarget() {
        return target;
    }

    @Override
    public T getBanTarget() {
        return banTarget;
    }

    @Override
    public Date getCreated() {
        return created == null ? null : (Date) created.clone();
    }

    @Override
    public void setCreated(Date created) {
        this.created = created;
    }

    @Override
    public String getSource() {
        return source;
    }

    @Override
    public void setSource(String source) {
        this.source = source;
    }

    @Override
    public Date getExpiration() {
        return expiration == null ? null : (Date) expiration.clone();
    }

    @Override
    public void setExpiration(Date expiration) {
        this.expiration = expiration;
    }

    @Override
    public String getReason() {
        return reason;
    }

    @Override
    public void setReason(String reason) {
        this.reason = reason;
    }

    @Override
    public void save() {
        write(target, id, created, source, expiration, reason);
    }

    @Override
    public void remove() {
        pardon(target, id);
    }
}
