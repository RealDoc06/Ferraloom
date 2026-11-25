package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubPlayer;
import java.util.Set;
import java.util.UUID;
import net.md_5.bungee.api.ChatMessageType;
import net.md_5.bungee.api.chat.BaseComponent;
import net.md_5.bungee.api.chat.TextComponent;
import org.bukkit.Location;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.event.player.PlayerTeleportEvent.TeleportCause;
import org.bukkit.permissions.PermissibleBase;
import org.bukkit.permissions.Permission;
import org.bukkit.permissions.PermissionAttachment;
import org.bukkit.permissions.PermissionAttachmentInfo;
import org.bukkit.plugin.Plugin;

/**
 * A connected player, as long as they stay connected. A player who leaves
 * keeps this object, offline for good (a reconnect gets a new one, as in
 * CraftBukkit): its name and UUID still answer, its location is where they
 * left, messages to it are dropped and teleports fail.
 */
final class FerraloomPlayer extends StubPlayer {
    final FerraloomServer server;
    final UUID id;
    final String name;
    final int entityId;
    private final boolean op;
    private final PermissibleBase permissions;
    volatile boolean online = true;
    /** Where they were when they left; set as the quit event fires. */
    private volatile Location left;
    private final Spigot spigot = new Spigot() {
        @Override
        public void sendMessage(BaseComponent component) {
            FerraloomPlayer.this.sendMessage(component.toLegacyText());
        }

        @Override
        public void sendMessage(BaseComponent... components) {
            FerraloomPlayer.this.sendMessage(TextComponent.toLegacyText(components));
        }

        @Override
        public void sendMessage(ChatMessageType position, BaseComponent component) {
            sendMessage(component);
        }

        @Override
        public void sendMessage(ChatMessageType position, BaseComponent... components) {
            sendMessage(components);
        }

        @Override
        public void sendMessage(ChatMessageType position, UUID sender, BaseComponent component) {
            sendMessage(component);
        }

        @Override
        public void sendMessage(ChatMessageType position, UUID sender, BaseComponent... components) {
            sendMessage(components);
        }
    };

    FerraloomPlayer(FerraloomServer server, UUID id, String name, int entityId, boolean op) {
        this.server = server;
        this.id = id;
        this.name = name;
        this.entityId = entityId;
        this.op = op;
        this.permissions = new PermissibleBase(this);
    }

    void leaving(Location at) {
        left = at;
    }

    // Identity.

    @Override
    public String getName() {
        return name;
    }

    @Override
    public String getDisplayName() {
        return name;
    }

    @Override
    public String getPlayerListName() {
        return name;
    }

    @Override
    public UUID getUniqueId() {
        return id;
    }

    @Override
    public int getEntityId() {
        return entityId;
    }

    @Override
    public EntityType getType() {
        return EntityType.PLAYER;
    }

    @Override
    public Server getServer() {
        return server;
    }

    @Override
    public boolean isOnline() {
        return online;
    }

    @Override
    public boolean isValid() {
        return online;
    }

    // Messages.

    @Override
    public void sendMessage(String message) {
        if (online && message != null) {
            PlayerNative.sendMessage(id.getMostSignificantBits(), id.getLeastSignificantBits(), message);
        }
    }

    @Override
    public void sendMessage(String... messages) {
        for (String message : messages) {
            sendMessage(message);
        }
    }

    @Override
    public void sendMessage(UUID sender, String message) {
        sendMessage(message);
    }

    @Override
    public void sendMessage(UUID sender, String... messages) {
        sendMessage(messages);
    }

    @Override
    public Spigot spigot() {
        return spigot;
    }

    // Position.

    @Override
    public Location getLocation() {
        Location gone = left;
        if (gone != null || !online) {
            return gone == null ? null : gone.clone();
        }
        double[] at = PlayerNative.location(id.getMostSignificantBits(), id.getLeastSignificantBits());
        if (at == null) {
            return null;
        }
        return new Location(server.world((int) at[0]), at[1], at[2], at[3], (float) at[4], (float) at[5]);
    }

    @Override
    public Location getLocation(Location into) {
        Location at = getLocation();
        if (into == null || at == null) {
            return at;
        }
        into.setWorld(at.getWorld());
        into.setX(at.getX());
        into.setY(at.getY());
        into.setZ(at.getZ());
        into.setYaw(at.getYaw());
        into.setPitch(at.getPitch());
        return into;
    }

    @Override
    public World getWorld() {
        Location at = getLocation();
        return at == null ? null : at.getWorld();
    }

    @Override
    public boolean teleport(Location location) {
        return teleport(location, TeleportCause.PLUGIN);
    }

    @Override
    public boolean teleport(Location location, TeleportCause cause) {
        if (!online || location == null) {
            return false;
        }
        World world = location.getWorld() == null ? getWorld() : location.getWorld();
        if (!(world instanceof FerraloomWorld w)) {
            return false;
        }
        return PlayerNative.teleport(id.getMostSignificantBits(), id.getLeastSignificantBits(), w.index,
                location.getX(), location.getY(), location.getZ(), location.getYaw(), location.getPitch());
    }

    @Override
    public boolean teleport(Entity destination) {
        return teleport(destination.getLocation());
    }

    @Override
    public boolean teleport(Entity destination, TeleportCause cause) {
        return teleport(destination.getLocation(), cause);
    }

    // Permissions: Bukkit's PermissibleBase over the server's operator list (read at join).

    @Override
    public boolean isOp() {
        return op;
    }

    @Override
    public boolean isPermissionSet(String name) {
        return permissions.isPermissionSet(name);
    }

    @Override
    public boolean isPermissionSet(Permission perm) {
        return permissions.isPermissionSet(perm);
    }

    @Override
    public boolean hasPermission(String name) {
        return permissions.hasPermission(name);
    }

    @Override
    public boolean hasPermission(Permission perm) {
        return permissions.hasPermission(perm);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value) {
        return permissions.addAttachment(plugin, name, value);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin) {
        return permissions.addAttachment(plugin);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value, int ticks) {
        return permissions.addAttachment(plugin, name, value, ticks);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, int ticks) {
        return permissions.addAttachment(plugin, ticks);
    }

    @Override
    public void removeAttachment(PermissionAttachment attachment) {
        permissions.removeAttachment(attachment);
    }

    @Override
    public void recalculatePermissions() {
        permissions.recalculatePermissions();
    }

    @Override
    public Set<PermissionAttachmentInfo> getEffectivePermissions() {
        return permissions.getEffectivePermissions();
    }

    // Object: CraftPlayer's identity is the UUID.

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomPlayer p && p.id.equals(id);
    }

    @Override
    public int hashCode() {
        return id.hashCode();
    }

    @Override
    public String toString() {
        return "FerraloomPlayer{name=" + name + "}";
    }
}
