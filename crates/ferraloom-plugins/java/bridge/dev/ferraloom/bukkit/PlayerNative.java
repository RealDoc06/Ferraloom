package dev.ferraloom.bukkit;

/**
 * The players area's calls into the server (src/players.rs, same names and signatures). Players are named by
 * their UUID's halves. Every call but {@link #sendMessage}, {@link #state} and {@link #location} needs the server
 * thread; those three answer elsewhere too (a message is queued, the others read the tick's snapshot).
 */
final class PlayerNative {
    private PlayerNative() {
    }

    // State: indices into state()'s answer (PlayerState::to_array).
    static final int HEALTH = 0, MAX_HEALTH = 1, ABSORPTION = 2, FOOD = 3, SATURATION = 4, EXHAUSTION = 5, LEVEL = 6,
            EXP = 7, TOTAL_EXP = 8, GAME_MODE = 9, FLYING = 10, ALLOW_FLIGHT = 11, WALK_SPEED = 12, FLY_SPEED = 13,
            SNEAKING = 14, SPRINTING = 15, ON_GROUND = 16, SWIMMING = 17, GLIDING = 18, SLEEPING = 19, DEAD = 20,
            IN_VEHICLE = 21, VELOCITY = 22, FIRE_TICKS = 25, AIR = 26, MAX_AIR = 27, FALL_DISTANCE = 28,
            FROZEN_TICKS = 29, PING = 30, TICKS_LIVED = 31, HELD_SLOT = 32, OP_LEVEL = 33, EYE_HEIGHT = 34, WIDTH = 35,
            HEIGHT = 36, RESPAWN = 37, STATE_LENGTH = 41;

    // Fields: set()'s field (Field::ALL's order).
    static final int SET_HEALTH = 0, SET_FOOD = 1, SET_SATURATION = 2, SET_EXHAUSTION = 3, SET_LEVEL = 4, SET_EXP = 5,
            SET_TOTAL_EXP = 6, GIVE_EXP = 7, GIVE_LEVELS = 8, SET_GAME_MODE = 9, SET_FLYING = 10, SET_FIRE_TICKS = 11,
            SET_AIR = 12, SET_FALL_DISTANCE = 13, SET_SNEAKING = 14, SET_SPRINTING = 15, SET_HELD_SLOT = 16,
            SET_ABSORPTION = 17;

    /** A chat line (legacy {@code §} codes) to one online player; from any thread. */
    static native void sendMessage(long most, long least, String message);

    /** {world, x, y, z, yaw, pitch} of an online player, or null when they are not on. */
    static native double[] location(long most, long least);

    /** Moves an online player, across worlds too; false when it could not. Fires no event. */
    static native boolean teleport(long most, long least, int world, double x, double y, double z, float yaw, float pitch);

    /** The player's state (the indices above), or null when they are not on. */
    static native double[] state(long most, long least);

    /** Sets one field (the SET_ constants); false when it could not. */
    static native boolean set(long most, long least, int field, double value);

    static native boolean setVelocity(long most, long least, double x, double y, double z);

    /** Legacy text over the hotbar. */
    static native boolean actionBar(long most, long least, String text);

    /** CraftPlayer.sendTitle: the times, then the title and the subtitle, each when not null. */
    static native boolean title(long most, long least, String title, String subtitle, int fadeIn, int stay, int fadeOut);

    static native boolean resetTitle(long most, long least);

    static native boolean tabList(long most, long least, String header, String footer);

    /** A sound event by name at a place; category is SoundCategory's ordinal. */
    static native boolean sound(long most, long least, String sound, int category, double x, double y, double z,
            float volume, float pitch, long seed);

    /** Stops a sound (null: any) in a category (-1: any). */
    static native boolean stopSound(long most, long least, String sound, int category);

    /** A particle type without data. */
    static native boolean particle(long most, long least, String particle, double x, double y, double z, int count,
            float dx, float dy, float dz, float speed, boolean force);

    /** A block state ("minecraft:stone") the client is told stands there. */
    static native boolean blockChange(long most, long least, int x, int y, int z, String state);

    static native boolean pluginMessage(long most, long least, String channel, byte[] data);

    /** The player list's name for the player; null: their own. */
    static native boolean listName(long most, long least, String name);

    static native boolean updateCommands(long most, long least);

    /** Each active effect as "id duration amplifier ambient particles icon". */
    static native String[] effects(long most, long least);

    static native boolean addEffect(long most, long least, String id, int duration, int amplifier, boolean ambient,
            boolean particles, boolean icon);

    static native boolean removeEffect(long most, long least, String id);

    /** Disconnects the player (legacy reason). Fires no event. */
    static native boolean kick(long most, long least, String reason);

    /** A vanilla command line (no slash) as the player, at their permission level. */
    static native boolean command(long most, long least, String line);

    /** {ip, port, locale, client view distance}, or null when they are not on. */
    static native String[] client(long most, long least);
}
