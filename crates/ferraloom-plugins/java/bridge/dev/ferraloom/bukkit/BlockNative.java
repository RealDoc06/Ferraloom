package dev.ferraloom.bukkit;

/**
 * The blocks area's calls into the server (src/blocks.rs). Worlds are named by their index in
 * {@link Bridge#start}'s list; blocks by their global state id ({@link Blocks}).
 */
final class BlockNative {
    private BlockNative() {
    }

    /** Every block, one line each (src/blocks.rs, {@code table_line}); asked once. */
    static native String table();

    /** The state id at a position, or -1 where no chunk is loaded. */
    static native int state(int world, int x, int y, int z);

    /** CraftBlock.setTypeAndData: false when nothing changed or no chunk is loaded there. */
    static native boolean setState(int world, int x, int y, int z, int state, boolean physics);

    /** {@code sky << 8 | block << 4 | max}, or -1 where no chunk is loaded. */
    static native int light(int world, int x, int y, int z);

    /** The biome's key, or null where no chunk is loaded. */
    static native String biome(int world, int x, int y, int z);

    /** Level.destroyBlock(pos, true): false when there was nothing to break. */
    static native boolean breakNaturally(int world, int x, int y, int z);

    /** A sign's four lines on one side, or null where there is no sign. */
    static native String[] signLines(int world, int x, int y, int z, boolean front);

    /** Writes a sign's lines on one side: false where there is no sign. */
    static native boolean setSignLines(int world, int x, int y, int z, boolean front, String[] lines);
}
