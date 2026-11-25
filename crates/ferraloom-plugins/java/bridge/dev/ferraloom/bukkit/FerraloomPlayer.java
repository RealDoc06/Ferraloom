package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubPlayer;
import java.io.ByteArrayOutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ThreadLocalRandom;
import net.md_5.bungee.api.ChatMessageType;
import net.md_5.bungee.api.chat.BaseComponent;
import net.md_5.bungee.api.chat.TextComponent;
import org.bukkit.GameMode;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.Particle;
import org.bukkit.Registry;
import org.bukkit.Server;
import org.bukkit.Sound;
import org.bukkit.SoundCategory;
import org.bukkit.World;
import org.bukkit.block.BlockFace;
import org.bukkit.block.data.BlockData;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.Player;
import org.bukkit.entity.Pose;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.event.player.PlayerTeleportEvent.TeleportCause;
import org.bukkit.metadata.MetadataValue;
import org.bukkit.permissions.PermissibleBase;
import org.bukkit.permissions.Permission;
import org.bukkit.permissions.PermissionAttachment;
import org.bukkit.permissions.PermissionAttachmentInfo;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.messaging.StandardMessenger;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionEffectType;
import org.bukkit.util.BoundingBox;
import org.bukkit.util.Vector;

/**
 * A player from their login to their quit. The object is made for {@code PlayerLoginEvent} and goes online at
 * join; a player who leaves keeps it, offline for good (a reconnect gets a new one, as in CraftBukkit): its name
 * and UUID still answer, its location and state are as they left, messages to it are dropped and teleports fail.
 *
 * <p>State (health, game mode, experience...) is asked of the server at each call, one snapshot per getter
 * ({@link PlayerNative#state}); from another thread the snapshot is the tick's. What CraftPlayer keeps on the
 * Bukkit side (display name, tab list header and footer, the channels the client listens on) is kept here.
 */
final class FerraloomPlayer extends StubPlayer {
    /** A state before the player is in the world: a fresh survival player's. */
    private static final double[] FRESH = new double[PlayerNative.STATE_LENGTH];

    static {
        FRESH[PlayerNative.HEALTH] = 20;
        FRESH[PlayerNative.MAX_HEALTH] = 20;
        FRESH[PlayerNative.FOOD] = 20;
        FRESH[PlayerNative.SATURATION] = 5;
        FRESH[PlayerNative.WALK_SPEED] = 0.2;
        FRESH[PlayerNative.FLY_SPEED] = 0.1;
        FRESH[PlayerNative.AIR] = 300;
        FRESH[PlayerNative.MAX_AIR] = 300;
        FRESH[PlayerNative.EYE_HEIGHT] = 1.62;
        FRESH[PlayerNative.WIDTH] = 0.6;
        FRESH[PlayerNative.HEIGHT] = 1.8;
        FRESH[PlayerNative.RESPAWN] = -1;
    }

    final FerraloomServer server;
    final UUID id;
    final String name;
    private final long most;
    private final long least;
    volatile int entityId = -1;
    private final boolean op;
    private final PermissibleBase permissions;
    volatile boolean online;
    /** Where they were when they left; set as the quit event fires. */
    private volatile Location left;
    /** The last state read, which answers once they are gone. */
    private volatile double[] last = FRESH;
    private volatile boolean playedBefore;
    private volatile long firstPlayed;
    private volatile long lastPlayed;
    private volatile String displayName;
    private volatile String listName;
    private volatile String header;
    private volatile String footer;
    /** The plugin channels the client registered (minecraft:register), which sendPluginMessage sends on. */
    final Set<String> channels = ConcurrentHashMap.newKeySet();
    private final String metadata;
    private final Spigot spigot = new Spigot() {
        @Override
        public InetSocketAddress getRawAddress() {
            return getAddress();
        }

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
            sendMessage(position, new BaseComponent[] {component});
        }

        @Override
        public void sendMessage(ChatMessageType position, BaseComponent... components) {
            if (position == ChatMessageType.ACTION_BAR) {
                if (online) {
                    PlayerNative.actionBar(most, least, TextComponent.toLegacyText(components));
                }
            } else {
                sendMessage(components);
            }
        }

        @Override
        public void sendMessage(ChatMessageType position, UUID sender, BaseComponent component) {
            sendMessage(position, component);
        }

        @Override
        public void sendMessage(ChatMessageType position, UUID sender, BaseComponent... components) {
            sendMessage(position, components);
        }
    };

    /** A player logging in: not online until {@link #joined}. */
    FerraloomPlayer(FerraloomServer server, UUID id, String name, boolean op) {
        this.server = server;
        this.id = id;
        this.name = name;
        this.most = id.getMostSignificantBits();
        this.least = id.getLeastSignificantBits();
        this.op = op;
        this.permissions = new PermissibleBase(this);
        this.metadata = FerraloomMetadataStore.player(name);
    }

    /** PlayerList.placeNewPlayer: in the world, with what their player file said. */
    void joined(int entityId, boolean playedBefore, long firstPlayed, long lastPlayed) {
        this.entityId = entityId;
        this.playedBefore = playedBefore;
        this.firstPlayed = firstPlayed;
        this.lastPlayed = lastPlayed;
        this.online = true;
    }

    void leaving(Location at) {
        left = at;
        state();
        lastPlayed = System.currentTimeMillis();
    }

    /** The state now (the tick's off the server thread), or as last read once they are gone. */
    private double[] state() {
        if (online && left == null) {
            double[] s = PlayerNative.state(most, least);
            if (s != null) {
                last = s;
                return s;
            }
        }
        return last;
    }

    private double get(int index) {
        return state()[index];
    }

    private boolean flag(int index) {
        return state()[index] != 0;
    }

    /** Sets a field when the player is on; false otherwise. */
    private boolean set(int field, double value) {
        return online && left == null && PlayerNative.set(most, least, field, value);
    }

    // Identity.

    @Override
    public String getName() {
        return name;
    }

    @Override
    public String getDisplayName() {
        String shown = displayName;
        return shown == null ? name : shown;
    }

    /** CraftPlayer.setDisplayName: what chat formats and plugins show; null is the name. */
    @Override
    public void setDisplayName(String name) {
        displayName = name;
    }

    @Override
    public String getPlayerListName() {
        String shown = listName;
        return shown == null ? name : shown;
    }

    /** CraftPlayer.setPlayerListName: the name in everyone's tab list; null or empty is the name. */
    @Override
    public void setPlayerListName(String name) {
        listName = name == null || name.isEmpty() ? null : name;
        if (online) {
            PlayerNative.listName(most, least, listName);
        }
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
        return online && !isDead();
    }

    /** OfflinePlayer.getPlayer: this object while it is online. */
    @Override
    public Player getPlayer() {
        return online ? this : null;
    }

    @Override
    public boolean hasPlayedBefore() {
        return playedBefore;
    }

    @Override
    public long getFirstPlayed() {
        return firstPlayed;
    }

    @Override
    public long getLastPlayed() {
        return lastPlayed;
    }

    @Override
    public InetSocketAddress getAddress() {
        String[] c = client();
        return c == null ? null : new InetSocketAddress(c[0], Integer.parseInt(c[1]));
    }

    @Override
    public String getLocale() {
        String[] c = client();
        return c == null ? "en_us" : c[2];
    }

    @Override
    public int getClientViewDistance() {
        String[] c = client();
        return c == null ? 0 : Integer.parseInt(c[3]);
    }

    private String[] client() {
        return online && left == null ? PlayerNative.client(most, least) : null;
    }

    @Override
    public int getPing() {
        return (int) get(PlayerNative.PING);
    }

    // Messages and what the client shows.

    @Override
    public void sendMessage(String message) {
        if (online && message != null) {
            PlayerNative.sendMessage(most, least, message);
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
    public void sendRawMessage(String message) {
        sendMessage(message);
    }

    @Override
    public void sendRawMessage(UUID sender, String message) {
        sendMessage(message);
    }

    @Override
    public Spigot spigot() {
        return spigot;
    }

    /** The deprecated two-argument form: CraftPlayer's default times (10, 70, 20). */
    @Override
    public void sendTitle(String title, String subtitle) {
        sendTitle(title, subtitle, 10, 70, 20);
    }

    @Override
    public void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut) {
        if (online) {
            PlayerNative.title(most, least, title, subtitle, fadeIn, stay, fadeOut);
        }
    }

    @Override
    public void resetTitle() {
        if (online) {
            PlayerNative.resetTitle(most, least);
        }
    }

    @Override
    public String getPlayerListHeader() {
        return header;
    }

    @Override
    public String getPlayerListFooter() {
        return footer;
    }

    @Override
    public void setPlayerListHeader(String header) {
        setPlayerListHeaderFooter(header, footer);
    }

    @Override
    public void setPlayerListFooter(String footer) {
        setPlayerListHeaderFooter(header, footer);
    }

    /** CraftPlayer.updatePlayerListHeaderFooter: both, an unset one empty. */
    @Override
    public void setPlayerListHeaderFooter(String header, String footer) {
        this.header = header;
        this.footer = footer;
        if (online) {
            PlayerNative.tabList(most, least, header == null ? "" : header, footer == null ? "" : footer);
        }
    }

    @Override
    public void updateCommands() {
        if (online) {
            PlayerNative.updateCommands(most, least);
        }
    }

    // Sounds and particles.

    private static String key(Sound sound) {
        return sound.getKey().toString();
    }

    @Override
    public void playSound(Location location, Sound sound, float volume, float pitch) {
        playSound(location, sound, SoundCategory.MASTER, volume, pitch);
    }

    @Override
    public void playSound(Location location, String sound, float volume, float pitch) {
        playSound(location, sound, SoundCategory.MASTER, volume, pitch);
    }

    @Override
    public void playSound(Location location, Sound sound, SoundCategory category, float volume, float pitch) {
        playSound(location, sound, category, volume, pitch, ThreadLocalRandom.current().nextLong());
    }

    @Override
    public void playSound(Location location, String sound, SoundCategory category, float volume, float pitch) {
        playSound(location, sound, category, volume, pitch, ThreadLocalRandom.current().nextLong());
    }

    @Override
    public void playSound(Location location, Sound sound, SoundCategory category, float volume, float pitch,
            long seed) {
        if (location != null && sound != null) {
            playSound(location, key(sound), category, volume, pitch, seed);
        }
    }

    /** CraftPlayer.playSound: a sound event by name at the place, to this player only. */
    @Override
    public void playSound(Location location, String sound, SoundCategory category, float volume, float pitch,
            long seed) {
        if (location == null || sound == null || category == null || !online) {
            return;
        }
        PlayerNative.sound(most, least, NamespacedKey.fromString(sound).toString(), category.ordinal(),
                location.getX(), location.getY(), location.getZ(), volume, pitch, seed);
    }

    @Override
    public void playSound(Entity entity, Sound sound, float volume, float pitch) {
        playSound(entity, sound, SoundCategory.MASTER, volume, pitch);
    }

    @Override
    public void playSound(Entity entity, String sound, float volume, float pitch) {
        playSound(entity, sound, SoundCategory.MASTER, volume, pitch);
    }

    @Override
    public void playSound(Entity entity, Sound sound, SoundCategory category, float volume, float pitch) {
        playSound(entity, sound, category, volume, pitch, ThreadLocalRandom.current().nextLong());
    }

    @Override
    public void playSound(Entity entity, String sound, SoundCategory category, float volume, float pitch) {
        playSound(entity, sound, category, volume, pitch, ThreadLocalRandom.current().nextLong());
    }

    /** At the entity's place now (CraftPlayer sends ClientboundSoundEntityPacket, which follows it). */
    @Override
    public void playSound(Entity entity, Sound sound, SoundCategory category, float volume, float pitch, long seed) {
        if (entity != null) {
            playSound(entity.getLocation(), sound, category, volume, pitch, seed);
        }
    }

    @Override
    public void playSound(Entity entity, String sound, SoundCategory category, float volume, float pitch, long seed) {
        if (entity != null) {
            playSound(entity.getLocation(), sound, category, volume, pitch, seed);
        }
    }

    private void stop(String sound, SoundCategory category) {
        if (online) {
            String key = sound == null ? null : NamespacedKey.fromString(sound).toString();
            PlayerNative.stopSound(most, least, key, category == null ? -1 : category.ordinal());
        }
    }

    @Override
    public void stopSound(Sound sound) {
        stop(key(sound), null);
    }

    @Override
    public void stopSound(String sound) {
        stop(sound, null);
    }

    @Override
    public void stopSound(Sound sound, SoundCategory category) {
        stop(key(sound), category);
    }

    @Override
    public void stopSound(String sound, SoundCategory category) {
        stop(sound, category);
    }

    @Override
    public void stopSound(SoundCategory category) {
        stop(null, category);
    }

    @Override
    public void stopAllSounds() {
        stop(null, null);
    }

    @Override
    public void spawnParticle(Particle particle, Location location, int count) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count);
    }

    @Override
    public void spawnParticle(Particle particle, double x, double y, double z, int count) {
        spawnParticle(particle, x, y, z, count, null);
    }

    @Override
    public <T> void spawnParticle(Particle particle, Location location, int count, T data) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, data);
    }

    @Override
    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count, T data) {
        spawnParticle(particle, x, y, z, count, 0, 0, 0, data);
    }

    @Override
    public void spawnParticle(Particle particle, Location location, int count, double offsetX, double offsetY,
            double offsetZ) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ);
    }

    @Override
    public void spawnParticle(Particle particle, double x, double y, double z, int count, double offsetX,
            double offsetY, double offsetZ) {
        spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, null);
    }

    @Override
    public <T> void spawnParticle(Particle particle, Location location, int count, double offsetX, double offsetY,
            double offsetZ, T data) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ,
                data);
    }

    @Override
    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count, double offsetX,
            double offsetY, double offsetZ, T data) {
        spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, 1, data);
    }

    @Override
    public void spawnParticle(Particle particle, Location location, int count, double offsetX, double offsetY,
            double offsetZ, double extra) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ,
                extra);
    }

    @Override
    public void spawnParticle(Particle particle, double x, double y, double z, int count, double offsetX,
            double offsetY, double offsetZ, double extra) {
        spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, extra, null);
    }

    @Override
    public <T> void spawnParticle(Particle particle, Location location, int count, double offsetX, double offsetY,
            double offsetZ, double extra, T data) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ,
                extra, data);
    }

    @Override
    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count, double offsetX,
            double offsetY, double offsetZ, double extra, T data) {
        spawnParticle(particle, x, y, z, count, offsetX, offsetY, offsetZ, extra, data, false);
    }

    @Override
    public <T> void spawnParticle(Particle particle, Location location, int count, double offsetX, double offsetY,
            double offsetZ, double extra, T data, boolean force) {
        spawnParticle(particle, location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ,
                extra, data, force);
    }

    /**
     * CraftPlayer.spawnParticle: data must be of the particle's data type. Particles that carry data (dust, a
     * block, an item) are not bridged yet and throw.
     */
    @Override
    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count, double offsetX,
            double offsetY, double offsetZ, double extra, T data, boolean force) {
        if (data != null && !particle.getDataType().isInstance(data)) {
            throw new IllegalArgumentException("data (" + data.getClass() + ") should be " + particle.getDataType());
        }
        if (particle.getDataType() != Void.class) {
            throw new UnsupportedOperationException(
                    "Ferraloom does not implement Player.spawnParticle(...) yet for particles with data (" + particle + ")");
        }
        if (online) {
            PlayerNative.particle(most, least, particle.getKey().toString(), x, y, z, count, (float) offsetX,
                    (float) offsetY, (float) offsetZ, (float) extra, force);
        }
    }

    @Override
    public void sendBlockChange(Location location, BlockData block) {
        if (online && location != null && block != null) {
            PlayerNative.blockChange(most, least, location.getBlockX(), location.getBlockY(), location.getBlockZ(),
                    block.getAsString());
        }
    }

    // Plugin messages.

    /** CraftPlayer.sendPluginMessage: only on a channel the client registered. */
    @Override
    public void sendPluginMessage(Plugin source, String channel, byte[] message) {
        StandardMessenger.validatePluginMessage(server.getMessenger(), source, channel, message);
        if (online && channels.contains(channel)) {
            PlayerNative.pluginMessage(most, least, StandardMessenger.validateAndCorrectChannel(channel), message);
        }
    }

    @Override
    public Set<String> getListeningPluginChannels() {
        return Collections.unmodifiableSet(Set.copyOf(channels));
    }

    /** CraftPlayer.sendSupportedChannels: the channels plugins listen on, as minecraft:register. */
    void sendSupportedChannels() {
        Set<String> listening = server.getMessenger().getIncomingChannels();
        if (listening.isEmpty()) {
            return;
        }
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        for (String channel : listening) {
            out.writeBytes(channel.getBytes(StandardCharsets.UTF_8));
            out.write(0);
        }
        PlayerNative.pluginMessage(most, least, "minecraft:register", out.toByteArray());
    }

    // Health, food and experience.

    @Override
    public double getHealth() {
        return get(PlayerNative.HEALTH);
    }

    /** CraftLivingEntity.setHealth: between 0 and the maximum; 0 kills. */
    @Override
    public void setHealth(double health) {
        double max = getMaxHealth();
        if (health < 0 || health > max || Double.isNaN(health)) {
            throw new IllegalArgumentException("Health value (" + health + ") must be between 0 and " + max);
        }
        set(PlayerNative.SET_HEALTH, health);
    }

    @Override
    public double getMaxHealth() {
        return get(PlayerNative.MAX_HEALTH);
    }

    @Override
    public double getAbsorptionAmount() {
        return get(PlayerNative.ABSORPTION);
    }

    @Override
    public void setAbsorptionAmount(double amount) {
        if (amount < 0 || !Double.isFinite(amount)) {
            throw new IllegalArgumentException("amount < 0 or non finite");
        }
        set(PlayerNative.SET_ABSORPTION, amount);
    }

    /** Health is never scaled here (setHealthScale is not bridged): CraftPlayer's defaults. */
    @Override
    public boolean isHealthScaled() {
        return false;
    }

    @Override
    public double getHealthScale() {
        return 20;
    }

    @Override
    public int getFoodLevel() {
        return (int) get(PlayerNative.FOOD);
    }

    @Override
    public void setFoodLevel(int value) {
        set(PlayerNative.SET_FOOD, value);
    }

    @Override
    public float getSaturation() {
        return (float) get(PlayerNative.SATURATION);
    }

    @Override
    public void setSaturation(float value) {
        set(PlayerNative.SET_SATURATION, value);
    }

    @Override
    public float getExhaustion() {
        return (float) get(PlayerNative.EXHAUSTION);
    }

    @Override
    public void setExhaustion(float value) {
        set(PlayerNative.SET_EXHAUSTION, value);
    }

    @Override
    public float getExp() {
        return (float) get(PlayerNative.EXP);
    }

    @Override
    public void setExp(float exp) {
        if (exp < 0 || exp > 1) {
            throw new IllegalArgumentException("Experience progress must be between 0.0 and 1.0 (" + exp + ")");
        }
        set(PlayerNative.SET_EXP, exp);
    }

    @Override
    public int getLevel() {
        return (int) get(PlayerNative.LEVEL);
    }

    @Override
    public void setLevel(int level) {
        if (level < 0) {
            throw new IllegalArgumentException("Experience level must not be negative (" + level + ")");
        }
        set(PlayerNative.SET_LEVEL, level);
    }

    @Override
    public int getTotalExperience() {
        return (int) get(PlayerNative.TOTAL_EXP);
    }

    @Override
    public void setTotalExperience(int exp) {
        if (exp < 0) {
            throw new IllegalArgumentException("Total experience points must not be negative (" + exp + ")");
        }
        set(PlayerNative.SET_TOTAL_EXP, exp);
    }

    @Override
    public void giveExp(int amount) {
        set(PlayerNative.GIVE_EXP, amount);
    }

    @Override
    public void giveExpLevels(int amount) {
        set(PlayerNative.GIVE_LEVELS, amount);
    }

    /** Player.getXpNeededForNextLevel. */
    @Override
    public int getExpToLevel() {
        int level = getLevel();
        return level >= 30 ? 112 + (level - 30) * 9 : level >= 15 ? 37 + (level - 15) * 5 : 7 + level * 2;
    }

    // Game mode and movement.

    @Override
    public GameMode getGameMode() {
        return switch ((int) get(PlayerNative.GAME_MODE)) {
            case 1 -> GameMode.CREATIVE;
            case 2 -> GameMode.ADVENTURE;
            case 3 -> GameMode.SPECTATOR;
            default -> GameMode.SURVIVAL;
        };
    }

    /** ServerPlayer.setGameMode, which fires PlayerGameModeChangeEvent. */
    @Override
    public void setGameMode(GameMode mode) {
        if (mode == null) {
            throw new IllegalArgumentException("GameMode cannot be null");
        }
        int id = switch (mode) {
            case CREATIVE -> 1;
            case ADVENTURE -> 2;
            case SPECTATOR -> 3;
            default -> 0;
        };
        set(PlayerNative.SET_GAME_MODE, id);
    }

    @Override
    public boolean isFlying() {
        return flag(PlayerNative.FLYING);
    }

    @Override
    public void setFlying(boolean value) {
        if (value && !getAllowFlight()) {
            throw new IllegalArgumentException("Player is not allowed to fly (check #getAllowFlight())");
        }
        set(PlayerNative.SET_FLYING, value ? 1 : 0);
    }

    @Override
    public boolean getAllowFlight() {
        return flag(PlayerNative.ALLOW_FLIGHT);
    }

    @Override
    public float getWalkSpeed() {
        return (float) get(PlayerNative.WALK_SPEED);
    }

    @Override
    public float getFlySpeed() {
        return (float) get(PlayerNative.FLY_SPEED);
    }

    @Override
    public boolean isSneaking() {
        return flag(PlayerNative.SNEAKING);
    }

    @Override
    public void setSneaking(boolean sneak) {
        set(PlayerNative.SET_SNEAKING, sneak ? 1 : 0);
    }

    @Override
    public boolean isSprinting() {
        return flag(PlayerNative.SPRINTING);
    }

    @Override
    public void setSprinting(boolean sprinting) {
        set(PlayerNative.SET_SPRINTING, sprinting ? 1 : 0);
    }

    @Override
    public boolean isOnGround() {
        return flag(PlayerNative.ON_GROUND);
    }

    @Override
    public boolean isSwimming() {
        return flag(PlayerNative.SWIMMING);
    }

    @Override
    public boolean isGliding() {
        return flag(PlayerNative.GLIDING);
    }

    @Override
    public boolean isSleeping() {
        return flag(PlayerNative.SLEEPING);
    }

    @Override
    public boolean isDead() {
        return flag(PlayerNative.DEAD) || getHealth() <= 0;
    }

    @Override
    public boolean isInsideVehicle() {
        return flag(PlayerNative.IN_VEHICLE);
    }

    @Override
    public Pose getPose() {
        if (isSleeping()) {
            return Pose.SLEEPING;
        }
        if (isGliding()) {
            return Pose.FALL_FLYING;
        }
        if (isSwimming()) {
            return Pose.SWIMMING;
        }
        return isSneaking() && !isFlying() ? Pose.SNEAKING : Pose.STANDING;
    }

    @Override
    public Vector getVelocity() {
        double[] s = state();
        return new Vector(s[PlayerNative.VELOCITY], s[PlayerNative.VELOCITY + 1], s[PlayerNative.VELOCITY + 2]);
    }

    @Override
    public void setVelocity(Vector velocity) {
        if (velocity == null) {
            throw new IllegalArgumentException("velocity");
        }
        velocity.checkFinite();
        if (online) {
            PlayerNative.setVelocity(most, least, velocity.getX(), velocity.getY(), velocity.getZ());
        }
    }

    @Override
    public int getFireTicks() {
        return (int) get(PlayerNative.FIRE_TICKS);
    }

    @Override
    public void setFireTicks(int ticks) {
        set(PlayerNative.SET_FIRE_TICKS, ticks);
    }

    @Override
    public float getFallDistance() {
        return (float) get(PlayerNative.FALL_DISTANCE);
    }

    @Override
    public void setFallDistance(float distance) {
        set(PlayerNative.SET_FALL_DISTANCE, distance);
    }

    @Override
    public int getRemainingAir() {
        return (int) get(PlayerNative.AIR);
    }

    @Override
    public void setRemainingAir(int ticks) {
        set(PlayerNative.SET_AIR, ticks);
    }

    @Override
    public int getMaximumAir() {
        return (int) get(PlayerNative.MAX_AIR);
    }

    @Override
    public int getFreezeTicks() {
        return (int) get(PlayerNative.FROZEN_TICKS);
    }

    /** Entity.getTicksRequiredToFreeze. */
    @Override
    public int getMaxFreezeTicks() {
        return 140;
    }

    @Override
    public boolean isFrozen() {
        return getFreezeTicks() >= getMaxFreezeTicks();
    }

    @Override
    public int getTicksLived() {
        return (int) get(PlayerNative.TICKS_LIVED);
    }

    // Where they are.

    @Override
    public Location getLocation() {
        Location gone = left;
        if (gone != null || !online) {
            return gone == null ? null : gone.clone();
        }
        double[] at = PlayerNative.location(most, least);
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
    public double getEyeHeight() {
        return get(PlayerNative.EYE_HEIGHT);
    }

    /** Standing eyes when the pose is ignored. */
    @Override
    public double getEyeHeight(boolean ignorePose) {
        return ignorePose ? 1.62 : getEyeHeight();
    }

    @Override
    public Location getEyeLocation() {
        Location at = getLocation();
        return at == null ? null : at.add(0, getEyeHeight(), 0);
    }

    @Override
    public double getHeight() {
        return get(PlayerNative.HEIGHT);
    }

    @Override
    public double getWidth() {
        return get(PlayerNative.WIDTH);
    }

    @Override
    public BoundingBox getBoundingBox() {
        Location at = getLocation();
        double[] s = state();
        double half = s[PlayerNative.WIDTH] / 2;
        return new BoundingBox(at.getX() - half, at.getY(), at.getZ() - half, at.getX() + half,
                at.getY() + s[PlayerNative.HEIGHT], at.getZ() + half);
    }

    /** Entity.getMotionDirection: the horizontal direction the yaw faces. */
    @Override
    public BlockFace getFacing() {
        Location at = getLocation();
        int quarter = (int) Math.floor(at.getYaw() / 90.0 + 0.5) & 3;
        return new BlockFace[] {BlockFace.SOUTH, BlockFace.WEST, BlockFace.NORTH, BlockFace.EAST}[quarter];
    }

    /** CraftPlayer.getBedSpawnLocation: their own respawn point, none for the world spawn. */
    @Override
    public Location getBedSpawnLocation() {
        return getRespawnLocation();
    }

    @Override
    public Location getRespawnLocation() {
        double[] s = state();
        int world = (int) s[PlayerNative.RESPAWN];
        if (world < 0) {
            return null;
        }
        return new Location(server.world(world), s[PlayerNative.RESPAWN + 1], s[PlayerNative.RESPAWN + 2],
                s[PlayerNative.RESPAWN + 3]);
    }

    /** The world's clock: player time is never set apart here (setPlayerTime is not bridged). */
    @Override
    public long getPlayerTime() {
        World world = getWorld();
        return world == null ? 0 : world.getFullTime();
    }

    @Override
    public long getPlayerTimeOffset() {
        return 0;
    }

    @Override
    public boolean isPlayerTimeRelative() {
        return true;
    }

    @Override
    public void resetPlayerTime() {
    }

    /** The world's weather: never set apart here (setPlayerWeather is not bridged). */
    @Override
    public org.bukkit.WeatherType getPlayerWeather() {
        return null;
    }

    @Override
    public void resetPlayerWeather() {
    }

    /** Players are never hidden here (hidePlayer is not bridged). */
    @Override
    public boolean canSee(Player player) {
        return true;
    }

    @Override
    public boolean canSee(Entity entity) {
        return true;
    }

    @Override
    public boolean teleport(Location location) {
        return teleport(location, TeleportCause.PLUGIN);
    }

    /** CraftPlayer.teleport: PlayerTeleportEvent, then the move to where it leaves them. */
    @Override
    public boolean teleport(Location location, TeleportCause cause) {
        if (location == null) {
            throw new IllegalArgumentException("location cannot be null");
        }
        location.checkFinite();
        if (!online || left != null || isDead()) {
            return false;
        }
        Location from = getLocation();
        Location to = location.clone();
        if (to.getWorld() == null) {
            to.setWorld(from.getWorld());
        }
        if (PlayerTeleportEvent.getHandlerList().getRegisteredListeners().length > 0) {
            PlayerTeleportEvent event = new PlayerTeleportEvent(this, from, to, cause);
            server.getPluginManager().callEvent(event);
            if (event.isCancelled() || event.getTo() == null) {
                return false;
            }
            to = event.getTo();
        }
        return move(to);
    }

    /** Moves the player without an event. */
    boolean move(Location to) {
        World world = to.getWorld() == null ? getWorld() : to.getWorld();
        if (!(world instanceof FerraloomWorld w)) {
            return false;
        }
        return PlayerNative.teleport(most, least, w.index, to.getX(), to.getY(), to.getZ(), to.getYaw(),
                to.getPitch());
    }

    @Override
    public boolean teleport(Entity destination) {
        return teleport(destination.getLocation());
    }

    @Override
    public boolean teleport(Entity destination, TeleportCause cause) {
        return teleport(destination.getLocation(), cause);
    }

    // Effects.

    private static PotionEffectType effectType(String key) {
        return Registry.EFFECT.get(NamespacedKey.fromString(key));
    }

    @Override
    public boolean addPotionEffect(PotionEffect effect) {
        return addPotionEffect(effect, false);
    }

    /** CraftLivingEntity.addPotionEffect: `force` no longer matters (LivingEntity.addEffect replaces). */
    @Override
    public boolean addPotionEffect(PotionEffect effect, boolean force) {
        if (!online || left != null) {
            return false;
        }
        String key = effect.getType().getKey().toString();
        PlayerNative.addEffect(most, least, key, effect.getDuration(), effect.getAmplifier(), effect.isAmbient(),
                effect.hasParticles(), effect.hasIcon());
        return true;
    }

    @Override
    public boolean addPotionEffects(Collection<PotionEffect> effects) {
        boolean all = true;
        for (PotionEffect effect : effects) {
            all &= addPotionEffect(effect);
        }
        return all;
    }

    @Override
    public Collection<PotionEffect> getActivePotionEffects() {
        List<PotionEffect> active = new ArrayList<>();
        if (!online || left != null) {
            return active;
        }
        for (String line : PlayerNative.effects(most, least)) {
            String[] f = line.split(" ");
            PotionEffectType type = effectType(f[0]);
            if (type != null) {
                active.add(new PotionEffect(type, Integer.parseInt(f[1]), Integer.parseInt(f[2]), f[3].equals("1"),
                        f[4].equals("1"), f[5].equals("1")));
            }
        }
        return active;
    }

    @Override
    public PotionEffect getPotionEffect(PotionEffectType type) {
        for (PotionEffect effect : getActivePotionEffects()) {
            if (effect.getType().equals(type)) {
                return effect;
            }
        }
        return null;
    }

    @Override
    public boolean hasPotionEffect(PotionEffectType type) {
        return getPotionEffect(type) != null;
    }

    @Override
    public void removePotionEffect(PotionEffectType type) {
        if (online && left == null) {
            PlayerNative.removeEffect(most, least, type.getKey().toString());
        }
    }

    // What the player does.

    /** CraftPlayer.kickPlayer: PlayerKickEvent, then the disconnect. */
    @Override
    public void kickPlayer(String message) {
        PlayerEvents.kick(this, message == null ? "" : message);
    }

    /** CraftPlayer.chat: a command line through its preprocess event, else the chat events, as the player. */
    @Override
    public void chat(String message) {
        if (message.startsWith("/")) {
            PlayerEvents.command(this, message.substring(1));
        } else {
            PlayerEvents.say(this, message, false);
        }
    }

    /** CraftServer.dispatchCommand as the player: a plugin's command, else vanilla's at their level. */
    @Override
    public boolean performCommand(String command) {
        if (command == null) {
            throw new IllegalArgumentException("command cannot be null");
        }
        return server.dispatchPluginCommand(this, command) || (online && PlayerNative.command(most, least, command));
    }

    // Metadata (CraftPlayer's PlayerMetadataStore, by name).

    @Override
    public void setMetadata(String key, MetadataValue value) {
        FerraloomMetadataStore.set(metadata, key, value);
    }

    @Override
    public List<MetadataValue> getMetadata(String key) {
        return FerraloomMetadataStore.get(metadata, key);
    }

    @Override
    public boolean hasMetadata(String key) {
        return FerraloomMetadataStore.has(metadata, key);
    }

    @Override
    public void removeMetadata(String key, Plugin plugin) {
        FerraloomMetadataStore.remove(metadata, key, plugin);
    }

    // Permissions: Bukkit's PermissibleBase over the server's operator list (read at login).

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
