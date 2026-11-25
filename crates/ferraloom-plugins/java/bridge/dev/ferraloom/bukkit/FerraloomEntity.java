package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubEntity;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.util.Vector;

/**
 * Entity's methods for every entity but players: a handle (id, generation,
 * type) whose state is read from the server when asked. A plugin holds it
 * through its proxy ({@link Entities}); {@link #self} is that proxy.
 */
final class FerraloomEntity extends StubEntity {
    final int id;
    final int generation;
    final EntityType type;
    /** The Bukkit object plugins hold (the proxy over this). */
    Entity self;

    FerraloomEntity(int id, int generation, EntityType type) {
        this.id = id;
        this.generation = generation;
        this.type = type;
    }

    /** One read; null when the entity is gone. */
    double[] state() {
        return EntityNative.state(id, generation);
    }

    boolean flag(int bit) {
        double[] s = state();
        return s != null && ((int) s[EntityNative.FLAGS] & bit) != 0;
    }

    boolean change(int what, double... values) {
        return EntityNative.change(id, generation, what, values, null);
    }

    FerraloomWorld world(double[] s) {
        return Bridge.server.world((int) s[EntityNative.WORLD]);
    }

    @Override
    public int getEntityId() {
        return id;
    }

    @Override
    public EntityType getType() {
        return type;
    }

    /** Stable for the entity's life in this run (Ferraloom keeps no entity UUIDs yet). */
    @Override
    public UUID getUniqueId() {
        return UUID.nameUUIDFromBytes(("Ferraloom entity " + id + "/" + generation).getBytes(StandardCharsets.UTF_8));
    }

    @Override
    public Server getServer() {
        return Bridge.server;
    }

    /** Where it is; where it was last seen never: a gone entity answers null, as no CraftEntity is. */
    @Override
    public Location getLocation() {
        double[] s = state();
        if (s == null) {
            return null;
        }
        return new Location(world(s), s[EntityNative.X], s[EntityNative.Y], s[EntityNative.Z], (float) s[EntityNative.YAW],
                (float) s[EntityNative.PITCH]);
    }

    @Override
    public Location getLocation(Location into) {
        double[] s = state();
        if (into != null && s != null) {
            into.setWorld(world(s));
            into.setX(s[EntityNative.X]);
            into.setY(s[EntityNative.Y]);
            into.setZ(s[EntityNative.Z]);
            into.setYaw((float) s[EntityNative.YAW]);
            into.setPitch((float) s[EntityNative.PITCH]);
        }
        return into;
    }

    @Override
    public World getWorld() {
        double[] s = state();
        return s == null ? null : world(s);
    }

    @Override
    public Vector getVelocity() {
        double[] s = state();
        return s == null ? new Vector() : new Vector(s[EntityNative.VX], s[EntityNative.VY], s[EntityNative.VZ]);
    }

    @Override
    public void setVelocity(Vector v) {
        v.checkFinite();
        change(EntityNative.SET_VELOCITY, v.getX(), v.getY(), v.getZ());
    }

    @Override
    public boolean isOnGround() {
        return flag(EntityNative.ON_GROUND);
    }

    @Override
    public boolean teleport(Location to) {
        return teleport(to, PlayerTeleportEvent.TeleportCause.PLUGIN);
    }

    @Override
    public boolean teleport(Location to, PlayerTeleportEvent.TeleportCause cause) {
        if (!(to.getWorld() instanceof FerraloomWorld w)) {
            return false;
        }
        to.checkFinite();
        return change(EntityNative.TELEPORT, w.index, to.getX(), to.getY(), to.getZ(), to.getYaw(), to.getPitch());
    }

    @Override
    public boolean teleport(Entity to) {
        return teleport(to.getLocation());
    }

    @Override
    public boolean teleport(Entity to, PlayerTeleportEvent.TeleportCause cause) {
        return teleport(to.getLocation(), cause);
    }

    @Override
    public List<Entity> getNearbyEntities(double x, double y, double z) {
        Location at = getLocation();
        if (at == null) {
            return new ArrayList<>();
        }
        List<Entity> near = Entities.nearby((FerraloomWorld) at.getWorld(), at.getX() - x, at.getY() - y, at.getZ() - z,
                at.getX() + x, at.getY() + y, at.getZ() + z);
        near.remove(self);
        return near;
    }

    @Override
    public int getFireTicks() {
        double[] s = state();
        return s == null ? 0 : (int) s[EntityNative.FIRE];
    }

    @Override
    public int getMaxFireTicks() {
        return type.isAlive() ? 20 : 1;
    }

    @Override
    public void setFireTicks(int ticks) {
        change(EntityNative.SET_FIRE, ticks);
    }

    @Override
    public float getFallDistance() {
        double[] s = state();
        return s == null ? 0 : (float) s[EntityNative.FALL];
    }

    @Override
    public int getTicksLived() {
        double[] s = state();
        return s == null ? 0 : (int) s[EntityNative.TICKS_LIVED];
    }

    @Override
    public void remove() {
        change(EntityNative.REMOVE);
    }

    /** CraftEntity.isDead: !isAlive(), so a removed entity is dead too. */
    @Override
    public boolean isDead() {
        double[] s = state();
        return s == null || ((int) s[EntityNative.FLAGS] & EntityNative.DEAD) != 0;
    }

    @Override
    public boolean isValid() {
        return !isDead();
    }

    @Override
    public boolean isInWorld() {
        return state() != null;
    }

    @Override
    public boolean isPersistent() {
        return flag(EntityNative.PERSISTENT);
    }

    @Override
    public void setPersistent(boolean persistent) {
        change(EntityNative.SET_PERSISTENT, persistent ? 1 : 0);
    }

    @Override
    public String getCustomName() {
        return EntityNative.customName(id, generation);
    }

    @Override
    public void setCustomName(String name) {
        EntityNative.change(id, generation, EntityNative.SET_CUSTOM_NAME, null, name == null || name.isEmpty() ? null : name);
    }

    @Override
    public boolean isCustomNameVisible() {
        return flag(EntityNative.NAME_VISIBLE);
    }

    @Override
    public void setCustomNameVisible(boolean visible) {
        change(EntityNative.SET_NAME_VISIBLE, visible ? 1 : 0);
    }

    /** CraftEntity.getName: the custom name, else the type's name. */
    @Override
    public String getName() {
        String custom = getCustomName();
        return custom != null ? custom : Entities.simpleName(type);
    }

    @Override
    public boolean isGlowing() {
        return flag(EntityNative.GLOWING);
    }

    @Override
    public void setGlowing(boolean glowing) {
        change(EntityNative.SET_GLOWING, glowing ? 1 : 0);
    }

    @Override
    public boolean isInvulnerable() {
        return flag(EntityNative.INVULNERABLE);
    }

    @Override
    public void setInvulnerable(boolean invulnerable) {
        change(EntityNative.SET_INVULNERABLE, invulnerable ? 1 : 0);
    }

    @Override
    public boolean isSilent() {
        return flag(EntityNative.SILENT);
    }

    @Override
    public void setSilent(boolean silent) {
        change(EntityNative.SET_SILENT, silent ? 1 : 0);
    }

    @Override
    public boolean hasGravity() {
        return flag(EntityNative.GRAVITY);
    }

    @Override
    public void setGravity(boolean gravity) {
        change(EntityNative.SET_GRAVITY, gravity ? 1 : 0);
    }

    @Override
    public List<Entity> getPassengers() {
        Object found = EntityNative.related(id, generation, EntityNative.PASSENGERS);
        return found instanceof Entity[] all ? new ArrayList<>(Arrays.asList(all)) : new ArrayList<>();
    }

    @Override
    public Entity getPassenger() {
        List<Entity> all = getPassengers();
        return all.isEmpty() ? null : all.get(0);
    }

    @Override
    public boolean isEmpty() {
        return getPassengers().isEmpty();
    }

    // Metadata: the shared store, keyed by the entity's UUID (EntityMetadataStore).

    @Override
    public void setMetadata(String key, org.bukkit.metadata.MetadataValue value) {
        FerraloomMetadataStore.STORE.setMetadata(FerraloomMetadataStore.entity(getUniqueId()), key, value);
    }

    @Override
    public List<org.bukkit.metadata.MetadataValue> getMetadata(String key) {
        return FerraloomMetadataStore.STORE.getMetadata(FerraloomMetadataStore.entity(getUniqueId()), key);
    }

    @Override
    public boolean hasMetadata(String key) {
        return FerraloomMetadataStore.STORE.hasMetadata(FerraloomMetadataStore.entity(getUniqueId()), key);
    }

    @Override
    public void removeMetadata(String key, org.bukkit.plugin.Plugin plugin) {
        FerraloomMetadataStore.STORE.removeMetadata(FerraloomMetadataStore.entity(getUniqueId()), key, plugin);
    }

    /** CraftEntity: an entity is no command target; what it is sent goes nowhere. */
    @Override
    public void sendMessage(String message) {
    }

    @Override
    public void sendMessage(String... messages) {
    }

    @Override
    public void sendMessage(UUID sender, String message) {
    }

    @Override
    public void sendMessage(UUID sender, String... messages) {
    }

    @Override
    public String toString() {
        return "Craft" + Entities.simpleName(type) + "{id=" + id + "}";
    }
}
