package dev.ferraloom.bukkit;

/** The worlds area's calls into the server (src/worlds.rs). Worlds are named by their index in {@link Bridge#start}'s list. */
final class WorldNative {
    private WorldNative() {
    }

    /** The world's game time ({@code full}) or time of day. */
    static native long worldTime(int world, boolean full);

    /** The block state's name at a position ({@code minecraft:stone}), or null where no chunk is loaded. */
    static native String blockAt(int world, int x, int y, int z);
}
