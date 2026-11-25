package dev.ferraloom.bukkit;

/**
 * The worlds area's calls into the server (src/worlds.rs). Worlds are named by their index in
 * {@link Bridge#start}'s list. {@link #query} and {@link #change} carry most of World's numbers, by code
 * (src/worlds.rs, {@code Query} and {@code Change}: the same values).
 */
final class WorldNative {
    private WorldNative() {
    }

    // query codes.
    static final int FULL_TIME = 0;
    static final int TIME = 1;
    static final int STORM = 2;
    static final int THUNDERING = 3;
    static final int RAIN_TIME = 4;
    static final int THUNDER_TIME = 5;
    static final int CLEAR_TIME = 6;
    static final int SPAWN_X = 7;
    static final int SPAWN_Y = 8;
    static final int SPAWN_Z = 9;
    static final int SEA_LEVEL = 10;
    static final int HIGHEST = 11;
    static final int CHUNK_LOADED = 12;
    static final int CHUNK_GENERATED = 13;
    static final int DIFFICULTY = 14;
    static final int SEED = 15;
    static final int GAME_TIME = 16;
    /** {@link #query}'s answer when the world cannot answer. */
    static final long NONE = Long.MIN_VALUE;

    // change codes.
    static final int SET_FULL_TIME = 0;
    static final int SET_TIME = 1;
    static final int SET_STORM = 2;
    static final int SET_THUNDERING = 3;
    static final int SET_RAIN_TIME = 4;
    static final int SET_THUNDER_TIME = 5;
    static final int SET_CLEAR_TIME = 6;
    static final int SET_SPAWN = 7;
    static final int SET_DIFFICULTY = 8;
    static final int LOAD_CHUNK = 9;
    static final int UNLOAD_CHUNK = 10;
    static final int TICKET = 11;
    static final int BORDER_CENTER = 12;
    static final int BORDER_SIZE = 13;
    static final int BORDER_DAMAGE_BUFFER = 14;
    static final int BORDER_DAMAGE_AMOUNT = 15;
    static final int BORDER_WARNING_DISTANCE = 16;
    static final int BORDER_WARNING_TIME = 17;
    static final int SAVE = 18;

    /** The day time, whole ({@code full}) or within its day. */
    static native long worldTime(int world, boolean full);

    static native long query(int world, int what, int a, int b, int c);

    static native boolean change(int world, int what, long a, long b, long c, long d);

    /** A game rule's value as text ({@code true}, {@code 3}), or its default; null for no such rule. */
    static native String gameRule(String name, boolean defaultValue);

    static native boolean setGameRule(String name, String value);

    static native String[] gameRules();

    /** The loaded chunks' x and z, flat. */
    static native int[] loadedChunks(int world);

    /** Centre x, z, size, damage buffer, damage amount, warning distance, warning time. */
    static native double[] border(int world);

    static native void playSound(int world, double x, double y, double z, String sound, int category, float volume,
            float pitch, long seed);

    static native void levelEvent(int world, int x, int y, int z, int id, int data);

    static native boolean explode(int world, double x, double y, double z, float power, boolean fire, boolean breakBlocks);

    static native boolean strikeLightning(int world, double x, double y, double z, boolean effect);
}
