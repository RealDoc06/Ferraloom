package dev.ferraloom.bukkit;

/** The players area's calls into the server (src/players.rs). Players are named by their UUID's halves. */
final class PlayerNative {
    private PlayerNative() {
    }

    /** A chat line (legacy {@code §} codes) to one online player; from any thread. */
    static native void sendMessage(long most, long least, String message);

    /** {world, x, y, z, yaw, pitch} of an online player, or null when they are not on. */
    static native double[] location(long most, long least);

    /** Moves an online player, across worlds too; false when it could not. */
    static native boolean teleport(long most, long least, int world, double x, double y, double z, float yaw, float pitch);
}
