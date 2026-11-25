package dev.ferraloom.bukkit;

/**
 * The entities area's calls into the server (src/entities.rs). An entity is
 * named by its id and generation; one whose entity is gone reads as null or
 * false. Inside an event fired from mob code, an entity that event does not
 * name may throw IllegalStateException (its host cannot see it).
 */
final class EntityNative {
    private EntityNative() {
    }

    // state()'s layout (src/entities.rs, state_values).
    static final int WORLD = 0;
    static final int X = 1;
    static final int Y = 2;
    static final int Z = 3;
    static final int YAW = 4;
    static final int PITCH = 5;
    static final int VX = 6;
    static final int VY = 7;
    static final int VZ = 8;
    static final int FLAGS = 9;
    static final int FIRE = 10;
    static final int TICKS_LIVED = 11;
    static final int FALL = 12;
    static final int HEALTH = 13;
    static final int MAX_HEALTH = 14;
    static final int NO_DAMAGE = 15;

    // FLAGS' bits (src/entities.rs, flags).
    static final int ON_GROUND = 1;
    static final int NAME_VISIBLE = 1 << 1;
    static final int SILENT = 1 << 2;
    static final int GLOWING = 1 << 3;
    static final int INVULNERABLE = 1 << 4;
    static final int GRAVITY = 1 << 5;
    static final int LIVING = 1 << 6;
    static final int DEAD = 1 << 7;
    static final int AI = 1 << 8;
    static final int PERSISTENT = 1 << 9;
    static final int TAMED = 1 << 10;
    static final int BABY = 1 << 11;
    static final int INVISIBLE = 1 << 12;

    // change()'s what (src/entities.rs, decode).
    static final int SET_VELOCITY = 0;
    static final int TELEPORT = 1;
    static final int REMOVE = 2;
    static final int SET_CUSTOM_NAME = 3;
    static final int SET_NAME_VISIBLE = 4;
    static final int SET_FIRE = 5;
    static final int SET_HEALTH = 6;
    static final int SET_AI = 7;
    static final int SET_PERSISTENT = 8;
    static final int SET_SILENT = 9;
    static final int SET_GLOWING = 10;
    static final int SET_GRAVITY = 11;
    static final int SET_INVULNERABLE = 12;
    static final int SET_TAMED = 13;
    static final int SET_BABY = 14;

    // related()'s which.
    static final int TARGET = 0;
    static final int KILLER = 1;
    static final int OWNER = 2;
    static final int PASSENGERS = 3;

    /** The entity's state (the layout above), or null when it is gone. */
    static native double[] state(int id, int generation);

    static native String customName(int id, int generation);

    /** Its target, killer or owner (an Entity, null for none), or its passengers (an Entity[]). */
    static native Object related(int id, int generation, int which);

    /** A change (the codes above); false when the entity is gone. */
    static native boolean change(int id, int generation, int what, double[] values, String text);

    /** Mob.setTarget: {@code kind} null for none, "player" for a player ({@code a}, {@code b} its UUID's halves), else an entity's type, id and generation. */
    static native boolean setTarget(int id, int generation, long a, long b, String kind);

    /** A world's entities, players included, within {minX, minY, minZ, maxX, maxY, maxZ} when given. */
    static native Object[] list(int world, double[] within);

    /** World.spawnEntity: the new entity, or null when it cannot spawn. */
    static native Object spawn(String kind, int world, double x, double y, double z, float yaw, float pitch);
}
