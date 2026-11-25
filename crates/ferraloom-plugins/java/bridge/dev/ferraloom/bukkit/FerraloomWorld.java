package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubWorld;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import org.bukkit.Chunk;
import org.bukkit.Difficulty;
import org.bukkit.Effect;
import org.bukkit.GameRule;
import org.bukkit.HeightMap;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Sound;
import org.bukkit.SoundCategory;
import org.bukkit.block.Biome;
import org.bukkit.block.Block;
import org.bukkit.block.data.BlockData;
import org.bukkit.entity.Entity;
import org.bukkit.entity.LightningStrike;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;

/**
 * One of the server's dimensions, named as CraftBukkit names them: {@code world}, {@code world_nether},
 * {@code world_the_end}. Every dimension answers, whether its level is in place or parked (src/worlds.rs);
 * a level that is not loaded reads as unloaded chunks.
 */
final class FerraloomWorld extends StubWorld {
    final FerraloomServer server;
    /** Its place in the list the server gave {@link Bridge#start}: how the natives name it. */
    final int index;
    final String name;
    final Environment environment;
    final UUID uid;
    private final FerraloomWorldBorder border = new FerraloomWorldBorder(this);
    /** Plugin chunk tickets, per chunk ({@code x << 32 | z}); server thread only. The server counts one per holder. */
    private final Map<Long, Set<Plugin>> tickets = new HashMap<>();
    /** Chunks {@code loadChunk} asked for, held until {@code unloadChunk}. */
    private final Set<Long> requested = new HashSet<>();

    FerraloomWorld(FerraloomServer server, int index, String name, Environment environment) {
        this.server = server;
        this.index = index;
        this.name = name;
        this.environment = environment;
        // Stable across restarts until uid.dat is read (CraftWorld reads it from the level folder).
        this.uid = UUID.nameUUIDFromBytes(("Ferraloom world " + name).getBytes(StandardCharsets.UTF_8));
        // The block table, at start, where the server lends itself (Bridge.start).
        Blocks.load();
    }

    private static long chunkKey(int x, int z) {
        return (long) x << 32 | z & 0xFFFFFFFFL;
    }

    private long query(int what) {
        return WorldNative.query(index, what, 0, 0, 0);
    }

    private boolean change(int what, long a) {
        return WorldNative.change(index, what, a, 0, 0, 0);
    }

    @Override
    public String getName() {
        return name;
    }

    @Override
    public UUID getUID() {
        return uid;
    }

    @Override
    public Environment getEnvironment() {
        return environment;
    }

    @Override
    public NamespacedKey getKey() {
        return NamespacedKey.minecraft(switch (environment) {
            case NETHER -> "the_nether";
            case THE_END -> "the_end";
            default -> "overworld";
        });
    }

    /** DimensionType's min_y: the vanilla dimension types. */
    @Override
    public int getMinHeight() {
        return environment == Environment.NORMAL ? -64 : 0;
    }

    /** DimensionType's min_y + height. */
    @Override
    public int getMaxHeight() {
        return environment == Environment.NORMAL ? 320 : 256;
    }

    /** DimensionType's logical_height. */
    @Override
    public int getLogicalHeight() {
        return switch (environment) {
            case NETHER -> 128;
            case THE_END -> 256;
            default -> 384;
        };
    }

    @Override
    public int getSeaLevel() {
        return (int) query(WorldNative.SEA_LEVEL);
    }

    @Override
    public long getSeed() {
        return query(WorldNative.SEED);
    }

    /** The dimension's folder, which holds its {@code region/} ({@code dimensions/minecraft/<dimension>} in the level). */
    @Override
    public File getWorldFolder() {
        String level = server.worlds.get(0).name;
        return new File(server.directory, level + "/dimensions/minecraft/" + getKey().getKey());
    }

    // Time and weather: every dimension reads the server's (ServerLevel's derived level data).

    @Override
    public long getFullTime() {
        return WorldNative.worldTime(index, true);
    }

    @Override
    public long getTime() {
        return WorldNative.worldTime(index, false);
    }

    @Override
    public long getGameTime() {
        return query(WorldNative.GAME_TIME);
    }

    @Override
    public void setFullTime(long time) {
        change(WorldNative.SET_FULL_TIME, time);
    }

    @Override
    public void setTime(long time) {
        change(WorldNative.SET_TIME, time);
    }

    @Override
    public boolean hasStorm() {
        return query(WorldNative.STORM) != 0;
    }

    @Override
    public void setStorm(boolean storm) {
        change(WorldNative.SET_STORM, storm ? 1 : 0);
    }

    @Override
    public boolean isThundering() {
        return query(WorldNative.THUNDERING) != 0;
    }

    @Override
    public void setThundering(boolean thundering) {
        change(WorldNative.SET_THUNDERING, thundering ? 1 : 0);
    }

    @Override
    public int getWeatherDuration() {
        return (int) query(WorldNative.RAIN_TIME);
    }

    @Override
    public void setWeatherDuration(int duration) {
        change(WorldNative.SET_RAIN_TIME, duration);
    }

    @Override
    public int getThunderDuration() {
        return (int) query(WorldNative.THUNDER_TIME);
    }

    @Override
    public void setThunderDuration(int duration) {
        change(WorldNative.SET_THUNDER_TIME, duration);
    }

    @Override
    public boolean isClearWeather() {
        return !hasStorm() && !isThundering();
    }

    @Override
    public int getClearWeatherDuration() {
        return (int) query(WorldNative.CLEAR_TIME);
    }

    @Override
    public void setClearWeatherDuration(int duration) {
        change(WorldNative.SET_CLEAR_TIME, duration);
    }

    // The spawn: the server's respawn position, for every dimension (LevelData.getRespawnData).

    @Override
    public Location getSpawnLocation() {
        long x = query(WorldNative.SPAWN_X);
        if (x == WorldNative.NONE) {
            return new Location(this, 0, getSeaLevel() + 1, 0);
        }
        return new Location(this, x, query(WorldNative.SPAWN_Y), query(WorldNative.SPAWN_Z));
    }

    @Override
    public boolean setSpawnLocation(Location location) {
        if (location.getWorld() != null && !equals(location.getWorld())) {
            return false;
        }
        return setSpawnLocation(location.getBlockX(), location.getBlockY(), location.getBlockZ(), location.getYaw());
    }

    @Override
    public boolean setSpawnLocation(int x, int y, int z, float angle) {
        return WorldNative.change(index, WorldNative.SET_SPAWN, x, y, z, Float.floatToRawIntBits(angle));
    }

    @Override
    public boolean setSpawnLocation(int x, int y, int z) {
        return setSpawnLocation(x, y, z, 0.0F);
    }

    @Override
    public Difficulty getDifficulty() {
        return Difficulty.values()[(int) query(WorldNative.DIFFICULTY)];
    }

    @Override
    public void setDifficulty(Difficulty difficulty) {
        change(WorldNative.SET_DIFFICULTY, difficulty.ordinal());
    }

    // Game rules: the server's, shared by every dimension.

    @Override
    public String[] getGameRules() {
        return WorldNative.gameRules();
    }

    @Override
    public boolean isGameRule(String rule) {
        return WorldNative.gameRule(rule, true) != null;
    }

    @Override
    public <T> T getGameRuleValue(GameRule<T> rule) {
        return rule(rule).parse(WorldNative.gameRule(rule.getKey().getKey(), false));
    }

    @Override
    public <T> T getGameRuleDefault(GameRule<T> rule) {
        return rule(rule).parse(WorldNative.gameRule(rule.getKey().getKey(), true));
    }

    @Override
    public <T> boolean setGameRule(GameRule<T> rule, T value) {
        if (value == null) {
            return false;
        }
        return WorldNative.setGameRule(rule.getKey().getKey(), value.toString());
    }

    @SuppressWarnings("unchecked")
    private static <T> FerraloomGameRule<T> rule(GameRule<T> rule) {
        if (rule instanceof FerraloomGameRule<?> r) {
            return (FerraloomGameRule<T>) r;
        }
        throw new IllegalArgumentException("Not a game rule of this server: " + rule);
    }

    @Override
    public org.bukkit.WorldBorder getWorldBorder() {
        return border;
    }

    // Blocks.

    @Override
    public Block getBlockAt(int x, int y, int z) {
        return new FerraloomBlock(this, x, y, z);
    }

    @Override
    public Block getBlockAt(Location location) {
        return getBlockAt(location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    @Override
    public BlockData getBlockData(int x, int y, int z) {
        return getBlockAt(x, y, z).getBlockData();
    }

    @Override
    public BlockData getBlockData(Location location) {
        return getBlockAt(location).getBlockData();
    }

    @Override
    public Material getType(int x, int y, int z) {
        return getBlockAt(x, y, z).getType();
    }

    @Override
    public Material getType(Location location) {
        return getBlockAt(location).getType();
    }

    @Override
    public void setType(int x, int y, int z, Material type) {
        getBlockAt(x, y, z).setType(type);
    }

    @Override
    public void setType(Location location, Material type) {
        getBlockAt(location).setType(type);
    }

    @Override
    public void setBlockData(int x, int y, int z, BlockData data) {
        getBlockAt(x, y, z).setBlockData(data);
    }

    @Override
    public void setBlockData(Location location, BlockData data) {
        getBlockAt(location).setBlockData(data);
    }

    @Override
    public Biome getBiome(int x, int y, int z) {
        return getBlockAt(x, y, z).getBiome();
    }

    @Override
    public Biome getBiome(Location location) {
        return getBlockAt(location).getBiome();
    }

    /** CraftWorld.getHighestBlockYAt: MOTION_BLOCKING's height less one; the world's bottom where no chunk is loaded. */
    @Override
    public int getHighestBlockYAt(int x, int z) {
        return getHighestBlockYAt(x, z, HeightMap.MOTION_BLOCKING);
    }

    @Override
    public int getHighestBlockYAt(int x, int z, HeightMap heightMap) {
        long y = WorldNative.query(index, WorldNative.HIGHEST, x, z, heightMap.ordinal());
        return y == WorldNative.NONE ? getMinHeight() - 1 : (int) y;
    }

    @Override
    public int getHighestBlockYAt(Location location) {
        return getHighestBlockYAt(location.getBlockX(), location.getBlockZ());
    }

    @Override
    public int getHighestBlockYAt(Location location, HeightMap heightMap) {
        return getHighestBlockYAt(location.getBlockX(), location.getBlockZ(), heightMap);
    }

    @Override
    public Block getHighestBlockAt(int x, int z) {
        return getBlockAt(x, getHighestBlockYAt(x, z), z);
    }

    @Override
    public Block getHighestBlockAt(Location location) {
        return getHighestBlockAt(location.getBlockX(), location.getBlockZ());
    }

    @Override
    public Block getHighestBlockAt(int x, int z, HeightMap heightMap) {
        return getBlockAt(x, getHighestBlockYAt(x, z, heightMap), z);
    }

    @Override
    public Block getHighestBlockAt(Location location, HeightMap heightMap) {
        return getHighestBlockAt(location.getBlockX(), location.getBlockZ(), heightMap);
    }

    // Chunks. Loading is asynchronous: a chunk asked for loads over the next ticks (the server never
    // generates on its tick thread), and stays while a ticket holds it.

    @Override
    public Chunk getChunkAt(int x, int z) {
        return new FerraloomChunk(this, x, z);
    }

    @Override
    public Chunk getChunkAt(int x, int z, boolean generate) {
        return getChunkAt(x, z);
    }

    @Override
    public Chunk getChunkAt(Location location) {
        return getChunkAt(location.getBlockX() >> 4, location.getBlockZ() >> 4);
    }

    @Override
    public Chunk getChunkAt(Block block) {
        return getChunkAt(block.getX() >> 4, block.getZ() >> 4);
    }

    @Override
    public boolean isChunkLoaded(int x, int z) {
        return WorldNative.query(index, WorldNative.CHUNK_LOADED, x, z, 0) == 1;
    }

    @Override
    public boolean isChunkLoaded(Chunk chunk) {
        return isChunkLoaded(chunk.getX(), chunk.getZ());
    }

    @Override
    public boolean isChunkGenerated(int x, int z) {
        return WorldNative.query(index, WorldNative.CHUNK_GENERATED, x, z, 0) == 1;
    }

    @Override
    public Chunk[] getLoadedChunks() {
        int[] flat = WorldNative.loadedChunks(index);
        Chunk[] chunks = new Chunk[flat.length / 2];
        for (int i = 0; i < chunks.length; i++) {
            chunks[i] = new FerraloomChunk(this, flat[2 * i], flat[2 * i + 1]);
        }
        return chunks;
    }

    /** Asks for the chunk; true when it is loaded already (it loads over the next ticks otherwise). */
    @Override
    public boolean loadChunk(int x, int z, boolean generate) {
        if (isChunkLoaded(x, z)) {
            return true;
        }
        if (!generate && !isChunkGenerated(x, z)) {
            return false;
        }
        if (requested.add(chunkKey(x, z))) {
            WorldNative.change(index, WorldNative.LOAD_CHUNK, x, z, generate ? 1 : 0, 0);
        }
        return false;
    }

    @Override
    public void loadChunk(int x, int z) {
        loadChunk(x, z, true);
    }

    @Override
    public void loadChunk(Chunk chunk) {
        loadChunk(chunk.getX(), chunk.getZ(), true);
    }

    /** Lets go of a chunk {@link #loadChunk} asked for; players and tickets may still hold it. */
    @Override
    public boolean unloadChunkRequest(int x, int z) {
        if (requested.remove(chunkKey(x, z))) {
            WorldNative.change(index, WorldNative.UNLOAD_CHUNK, x, z, 0, 0);
        }
        return true;
    }

    @Override
    public boolean unloadChunk(int x, int z, boolean save) {
        return unloadChunkRequest(x, z);
    }

    @Override
    public boolean unloadChunk(int x, int z) {
        return unloadChunkRequest(x, z);
    }

    @Override
    public boolean unloadChunk(Chunk chunk) {
        return unloadChunkRequest(chunk.getX(), chunk.getZ());
    }

    @Override
    public boolean addPluginChunkTicket(int x, int z, Plugin plugin) {
        if (!plugin.isEnabled()) {
            throw new IllegalStateException("plugin is not enabled");
        }
        if (!tickets.computeIfAbsent(chunkKey(x, z), k -> new HashSet<>()).add(plugin)) {
            return false;
        }
        WorldNative.change(index, WorldNative.TICKET, x, z, 1, 0);
        return true;
    }

    @Override
    public boolean removePluginChunkTicket(int x, int z, Plugin plugin) {
        Set<Plugin> holders = tickets.get(chunkKey(x, z));
        if (holders == null || !holders.remove(plugin)) {
            return false;
        }
        if (holders.isEmpty()) {
            tickets.remove(chunkKey(x, z));
        }
        WorldNative.change(index, WorldNative.TICKET, x, z, 0, 0);
        return true;
    }

    /** Every ticket the plugin holds goes (JavaPluginLoader.disablePlugin calls this for every world). */
    @Override
    public void removePluginChunkTickets(Plugin plugin) {
        for (Long key : new ArrayList<>(tickets.keySet())) {
            removePluginChunkTicket((int) (key >> 32), (int) (long) key, plugin);
        }
    }

    @Override
    public Collection<Plugin> getPluginChunkTickets(int x, int z) {
        return List.copyOf(tickets.getOrDefault(chunkKey(x, z), Set.of()));
    }

    @Override
    public Map<Plugin, Collection<Chunk>> getPluginChunkTickets() {
        Map<Plugin, Collection<Chunk>> out = new HashMap<>();
        tickets.forEach((key, holders) -> {
            for (Plugin p : holders) {
                out.computeIfAbsent(p, k -> new ArrayList<>())
                        .add(new FerraloomChunk(this, (int) (key >> 32), (int) (long) key));
            }
        });
        return out;
    }

    // Effects, sounds, explosions, lightning.

    /** CraftWorld.playEffect: the effect's level event, with an int's data. */
    @Override
    public void playEffect(Location location, Effect effect, int data) {
        WorldNative.levelEvent(index, location.getBlockX(), location.getBlockY(), location.getBlockZ(), effect.getId(), data);
    }

    @Override
    public void playEffect(Location location, Effect effect, int data, int radius) {
        playEffect(location, effect, data);
    }

    /** CraftEffect.getDataValue for the data types without an int: a block's state for STEP_SOUND. */
    @Override
    public <T> void playEffect(Location location, Effect effect, T data) {
        int value;
        if (data == null) {
            value = 0;
        } else if (data instanceof Integer i) {
            value = i;
        } else if (data instanceof Material m && Blocks.kind(m) != null) {
            value = Blocks.kind(m).defaultState;
        } else if (data instanceof BlockData b) {
            value = Blocks.state(b);
        } else {
            throw new UnsupportedOperationException("Ferraloom does not implement World.playEffect with "
                    + data.getClass().getSimpleName() + " data yet");
        }
        playEffect(location, effect, value);
    }

    @Override
    public <T> void playEffect(Location location, Effect effect, T data, int radius) {
        playEffect(location, effect, data);
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
    public void playSound(Location location, Sound sound, SoundCategory category, float volume, float pitch, long seed) {
        playSound(location, sound.getKeyOrThrow().toString(), category, volume, pitch, seed);
    }

    /** ServerLevel.playSeededSound: the players of this world within the sound's range hear it. */
    @Override
    public void playSound(Location location, String sound, SoundCategory category, float volume, float pitch, long seed) {
        if (location == null || sound == null || category == null) {
            return;
        }
        WorldNative.playSound(index, location.getX(), location.getY(), location.getZ(), sound, category.ordinal(),
                volume, pitch, seed);
    }

    @Override
    public void playSound(Entity entity, Sound sound, float volume, float pitch) {
        playSound(entity.getLocation(), sound, volume, pitch);
    }

    @Override
    public void playSound(Entity entity, String sound, float volume, float pitch) {
        playSound(entity.getLocation(), sound, volume, pitch);
    }

    @Override
    public void playSound(Entity entity, Sound sound, SoundCategory category, float volume, float pitch) {
        playSound(entity.getLocation(), sound, category, volume, pitch);
    }

    @Override
    public void playSound(Entity entity, String sound, SoundCategory category, float volume, float pitch) {
        playSound(entity.getLocation(), sound, category, volume, pitch);
    }

    @Override
    public void playSound(Entity entity, Sound sound, SoundCategory category, float volume, float pitch, long seed) {
        playSound(entity.getLocation(), sound, category, volume, pitch, seed);
    }

    @Override
    public void playSound(Entity entity, String sound, SoundCategory category, float volume, float pitch, long seed) {
        playSound(entity.getLocation(), sound, category, volume, pitch, seed);
    }

    @Override
    public boolean createExplosion(double x, double y, double z, float power) {
        return createExplosion(x, y, z, power, false, true);
    }

    @Override
    public boolean createExplosion(double x, double y, double z, float power, boolean setFire) {
        return createExplosion(x, y, z, power, setFire, true);
    }

    /** CraftWorld.createExplosion with no source: a block explosion (BlockExplodeEvent). */
    @Override
    public boolean createExplosion(double x, double y, double z, float power, boolean setFire, boolean breakBlocks) {
        return WorldNative.explode(index, x, y, z, power, setFire, breakBlocks);
    }

    @Override
    public boolean createExplosion(Location loc, float power) {
        return createExplosion(loc.getX(), loc.getY(), loc.getZ(), power);
    }

    @Override
    public boolean createExplosion(Location loc, float power, boolean setFire) {
        return createExplosion(loc.getX(), loc.getY(), loc.getZ(), power, setFire);
    }

    @Override
    public boolean createExplosion(Location loc, float power, boolean setFire, boolean breakBlocks) {
        return createExplosion(loc.getX(), loc.getY(), loc.getZ(), power, setFire, breakBlocks);
    }

    /** The bolt strikes; its entity handle comes with the entities area (null until then). */
    @Override
    public LightningStrike strikeLightning(Location loc) {
        WorldNative.strikeLightning(index, loc.getX(), loc.getY(), loc.getZ(), false);
        return null;
    }

    @Override
    public LightningStrike strikeLightningEffect(Location loc) {
        WorldNative.strikeLightning(index, loc.getX(), loc.getY(), loc.getZ(), true);
        return null;
    }

    @Override
    public void save() {
        change(WorldNative.SAVE, 0);
    }

    @Override
    public List<Player> getPlayers() {
        List<Player> here = new ArrayList<>();
        for (Player p : server.getOnlinePlayers()) {
            if (equals(p.getWorld())) {
                here.add(p);
            }
        }
        return here;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomWorld w && w.index == index && w.server == server;
    }

    @Override
    public int hashCode() {
        return uid.hashCode();
    }

    @Override
    public String toString() {
        return "CraftWorld{name=" + name + "}";
    }

    // Entities area (Entities): the level in place's entities; another world's are not reachable yet.
    @Override public List<Entity> getEntities() { return Entities.inWorld(this); }
    @Override public List<org.bukkit.entity.LivingEntity> getLivingEntities() { List<org.bukkit.entity.LivingEntity> living = new ArrayList<>(); for (Entity e : getEntities()) { if (e instanceof org.bukkit.entity.LivingEntity l) { living.add(l); } } return living; }
    @Override @SuppressWarnings("unchecked") public <T extends Entity> Collection<T> getEntitiesByClass(Class<T> type) { List<T> found = new ArrayList<>(); for (Entity e : getEntities()) { if (type.isInstance(e)) { found.add((T) e); } } return found; }
    @Override public Collection<Entity> getNearbyEntities(Location at, double x, double y, double z) { return Entities.nearby(this, at.getX() - x, at.getY() - y, at.getZ() - z, at.getX() + x, at.getY() + y, at.getZ() + z); }
    @Override public Entity spawnEntity(Location at, org.bukkit.entity.EntityType type) { return Entities.spawn(this, at, type); }
    @Override public Entity spawnEntity(Location at, org.bukkit.entity.EntityType type, boolean randomizeData) { return Entities.spawn(this, at, type); }
    @Override public org.bukkit.entity.Item dropItem(Location at, org.bukkit.inventory.ItemStack stack) { return Entities.dropItem(this, at, stack, false); }
    @Override public org.bukkit.entity.Item dropItemNaturally(Location at, org.bukkit.inventory.ItemStack stack) { return Entities.dropItem(this, at, stack, true); }
}
