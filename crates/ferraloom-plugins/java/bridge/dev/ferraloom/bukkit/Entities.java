package dev.ferraloom.bukkit;

import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import org.bukkit.Location;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;

/**
 * Bukkit objects for entities other than players. Each is a proxy over its
 * type's own interface ({@link EntityType#getEntityClass}: {@code Zombie},
 * {@code Wolf}, {@code Item}...), so every {@code instanceof} a plugin tests
 * answers as on CraftBukkit, without a class per type. A call goes to the
 * first of {@link FerraloomMob}, {@link FerraloomLivingEntity} and
 * {@link FerraloomEntity} that declares the method; a default method runs as
 * the interface wrote it; anything else throws "Ferraloom does not implement
 * Zombie.isBaby() yet", as a stub does.
 *
 * <p>The entry points other areas use: {@link #inWorld}, {@link #inChunk},
 * {@link #nearby}, {@link #spawn}; {@link #of} makes the object of an entity
 * the server names.
 */
public final class Entities {
    private Entities() {
    }

    /** What a proxy answers to give its implementation back. */
    interface Handle {
        FerraloomEntity ferraloom();
    }

    private static final Map<String, EntityType> BY_KEY = new HashMap<>();

    static {
        for (EntityType type : EntityType.values()) {
            if (type.getKeyOrNull() != null) {
                BY_KEY.put(type.getKeyOrNull().getKey(), type);
            }
        }
    }

    static EntityType type(String key) {
        EntityType type = BY_KEY.get(key.startsWith("minecraft:") ? key.substring(10) : key);
        return type == null ? EntityType.UNKNOWN : type;
    }

    /**
     * The object of an actor as src/entities.rs passes it: {@code kind} null
     * for none, "player" for an online player ({@code a}, {@code b} its UUID's
     * halves), else an entity's type key with its id and generation.
     */
    public static Entity of(long a, long b, String kind) {
        if (kind == null) {
            return null;
        }
        if (kind.equals("player")) {
            return Bridge.server.online(new UUID(a, b));
        }
        return create((int) a, (int) b, type(kind));
    }

    static Entity create(int id, int generation, EntityType type) {
        FerraloomEntity core = new FerraloomEntity(id, generation, type);
        Class<? extends Entity> face = type.getEntityClass() == null ? Entity.class : type.getEntityClass();
        Entity proxy = (Entity) Proxy.newProxyInstance(Entities.class.getClassLoader(),
                new Class<?>[] {face, Handle.class}, new Dispatch(core));
        core.self = proxy;
        return proxy;
    }

    /** The implementation behind a Bukkit entity of ours, or null for a player or another's object. */
    static FerraloomEntity unwrap(Object entity) {
        return entity instanceof Handle h ? h.ferraloom() : null;
    }

    public static List<Entity> inWorld(FerraloomWorld world) {
        return list(world, null);
    }

    public static List<Entity> inChunk(FerraloomWorld world, int chunkX, int chunkZ) {
        double x = chunkX * 16.0;
        double z = chunkZ * 16.0;
        // Feet in the column [x, x + 16), as a chunk holds its entities.
        List<Entity> found = new ArrayList<>();
        for (Entity e : list(world, new double[] {x, -1e9, z, x + 16, 1e9, z + 16})) {
            Location at = e.getLocation();
            if (Math.floor(at.getX() / 16) == chunkX && Math.floor(at.getZ() / 16) == chunkZ) {
                found.add(e);
            }
        }
        return found;
    }

    /** The entities whose boxes meet the given box (World.getNearbyEntities). */
    public static List<Entity> nearby(FerraloomWorld world, double minX, double minY, double minZ, double maxX,
            double maxY, double maxZ) {
        return list(world, new double[] {minX, minY, minZ, maxX, maxY, maxZ});
    }

    private static List<Entity> list(FerraloomWorld world, double[] within) {
        Object[] found = EntityNative.list(world.index, within);
        List<Entity> out = new ArrayList<>(found.length);
        for (Object o : found) {
            if (o != null) {
                out.add((Entity) o);
            }
        }
        return out;
    }

    /** World.spawnEntity: CreatureSpawnEvent's reason is CUSTOM. */
    @SuppressWarnings("unchecked")
    public static <T extends Entity> T spawn(FerraloomWorld world, Location at, EntityType type) {
        if (type == null || type.getKeyOrNull() == null || !type.isSpawnable()) {
            throw new IllegalArgumentException("Cannot spawn an entity of type " + type);
        }
        Object made = EntityNative.spawn(type.getKeyOrNull().getKey(), world.index, at.getX(), at.getY(), at.getZ(),
                at.getYaw(), at.getPitch());
        if (made == null) {
            throw new IllegalArgumentException("Ferraloom cannot spawn " + type.getKeyOrNull() + " yet");
        }
        return (T) made;
    }

    /** "Zombie" for ZOMBIE, as CraftBukkit's class names read. */
    static String simpleName(EntityType type) {
        Class<?> c = type.getEntityClass();
        if (c != null) {
            return c.getSimpleName();
        }
        String n = type.name().toLowerCase(Locale.ROOT);
        return Character.toUpperCase(n.charAt(0)) + n.substring(1);
    }

    /** Routes a proxy's calls; see the class comment. */
    private static final class Dispatch implements InvocationHandler {
        /** For each interface method, the implementation's method and which object answers it. */
        private static final Map<Method, Target> TARGETS = new ConcurrentHashMap<>();
        private static final Target NONE = new Target(null, -1);
        private static final Class<?>[] LAYERS = {FerraloomMob.class, FerraloomLivingEntity.class, FerraloomEntity.class};

        private final FerraloomEntity core;
        /** LAYERS' objects for this entity; the living ones are null for an entity that is not alive. */
        private final Object[] layers;

        Dispatch(FerraloomEntity core) {
            this.core = core;
            boolean alive = core.type.isAlive();
            FerraloomLivingEntity living = alive ? new FerraloomLivingEntity(core) : null;
            this.layers = new Object[] {alive ? new FerraloomMob(core, living) : null, living, core};
        }

        private record Target(MethodHandle handle, int layer) {
        }

        private static Target find(Method m) {
            for (int i = 0; i < LAYERS.length; i++) {
                try {
                    Method impl = LAYERS[i].getDeclaredMethod(m.getName(), m.getParameterTypes());
                    if (Modifier.isPublic(impl.getModifiers()) && m.getReturnType().isAssignableFrom(impl.getReturnType())) {
                        return new Target(MethodHandles.lookup().unreflect(impl), i);
                    }
                } catch (NoSuchMethodException | IllegalAccessException e) {
                    // Not in this layer.
                }
            }
            return NONE;
        }

        @Override
        public Object invoke(Object proxy, Method m, Object[] args) throws Throwable {
            switch (m.getName()) {
                case "ferraloom":
                    if (m.getParameterCount() == 0) {
                        return core;
                    }
                    break;
                case "equals":
                    if (m.getParameterCount() == 1 && m.getParameterTypes()[0] == Object.class) {
                        FerraloomEntity other = unwrap(args[0]);
                        return other != null && other.id == core.id && other.generation == core.generation;
                    }
                    break;
                case "hashCode":
                    if (m.getParameterCount() == 0) {
                        return core.id;
                    }
                    break;
                case "toString":
                    if (m.getParameterCount() == 0) {
                        return core.toString();
                    }
                    break;
                default:
                    break;
            }
            Target t = TARGETS.computeIfAbsent(m, Dispatch::find);
            Object on = t.layer >= 0 ? layers[t.layer] : null;
            if (on != null) {
                Object[] all = new Object[(args == null ? 0 : args.length) + 1];
                all[0] = on;
                if (args != null) {
                    System.arraycopy(args, 0, all, 1, args.length);
                }
                return t.handle.invokeWithArguments(all);
            }
            if (m.isDefault()) {
                try {
                    return InvocationHandler.invokeDefault(proxy, m, args);
                } catch (InvocationTargetException e) {
                    throw e.getCause();
                }
            }
            throw new UnsupportedOperationException(
                    "Ferraloom does not implement " + simpleName(core.type) + "." + m.getName() + "() yet");
        }
    }
}
