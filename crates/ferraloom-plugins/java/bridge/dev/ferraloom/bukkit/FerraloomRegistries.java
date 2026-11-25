package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubEnchantment;
import dev.ferraloom.bukkit.stub.StubGameEvent;
import dev.ferraloom.bukkit.stub.StubMusicInstrument;
import dev.ferraloom.bukkit.stub.StubPotionEffectType;
import dev.ferraloom.bukkit.stub.StubStructure;
import dev.ferraloom.bukkit.stub.StubStructureType;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Function;
import java.util.stream.Stream;
import org.bukkit.GameEvent;
import org.bukkit.Keyed;
import org.bukkit.MusicInstrument;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.block.Biome;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.generator.structure.Structure;
import org.bukkit.generator.structure.StructureType;
import org.bukkit.potion.PotionEffectType;
import org.bukkit.util.OldEnum;

/**
 * {@link org.bukkit.Server#getRegistry}: one {@link Registry} per keyed type, as CraftRegistry holds.
 * {@code Registry}'s own static fields ask for about thirty of them at once, so every one must exist
 * before any plugin touches {@code Material.isAir()}, {@code Sound.X} or {@code Enchantment.X}.
 *
 * <p>The entries are the API's own constants: a registry's entries are the keys its type's static
 * fields ask for while that type initializes ({@code Sound.ENTITY_PIG_AMBIENT = getSound("entity.pig.ambient")}),
 * in field order, which is the API version's list of vanilla's registry. A registry asked for anything
 * before then initializes the type first; after it, the list is closed and an unknown key answers null.
 *
 * <p>An entry is made by its type's factory in {@link #FACTORIES}; a type without one gets a
 * {@link KeyOnly} proxy (interfaces) answering its key, name, ordinal and order, and throwing
 * {@code UnsupportedOperationException} for the rest. An area giving a type real behaviour adds one
 * line to {@link #FACTORIES}, in its own block: {@code FACTORIES.put(BlockType.class, FerraloomBlockType::new);}.
 */
final class FerraloomRegistries {
    /** Entry factories by registry type: {@code key -> entry}. Appends only, one per type, in the area's block. */
    private static final Map<Class<?>, Function<NamespacedKey, ? extends Keyed>> FACTORIES = new LinkedHashMap<>();

    static {
        // Abstract classes a proxy cannot implement: key-only entries until an area implements them.
        FACTORIES.put(Enchantment.class, Enchantments::new);
        FACTORIES.put(GameEvent.class, GameEvents::new);
        FACTORIES.put(MusicInstrument.class, Instruments::new);
        FACTORIES.put(PotionEffectType.class, Effects::new);
        FACTORIES.put(Structure.class, Structures::new);
        FACTORIES.put(StructureType.class, StructureTypes::new);
        // Blocks and worlds.
        // Entities.
        // Inventory.
        FACTORIES.put(org.bukkit.inventory.ItemType.class, FerraloomItemType::new);
        // Server.
    }

    /** {@code Biome.CUSTOM}: CraftBukkit's stand-in for a biome no API constant names, outside the registry. */
    static final Biome CUSTOM_BIOME = KeyOnly.factory(Biome.class).apply(NamespacedKey.minecraft("custom"));

    private static final Map<Class<?>, Registry<?>> REGISTRIES = new ConcurrentHashMap<>();

    private FerraloomRegistries() {
    }

    /** The registry of {@code type}, made on first ask. */
    @SuppressWarnings("unchecked")
    static <T extends Keyed> Registry<T> get(Class<T> type) {
        return (Registry<T>) REGISTRIES.computeIfAbsent(type, t -> new Constants<>(type, factory(type)));
    }

    @SuppressWarnings("unchecked")
    private static <T extends Keyed> Function<NamespacedKey, T> factory(Class<T> type) {
        Function<NamespacedKey, ? extends Keyed> f = FACTORIES.get(type);
        if (f != null) {
            return (Function<NamespacedKey, T>) f;
        }
        if (type.isInterface()) {
            return KeyOnly.factory(type);
        }
        return key -> {
            throw new UnsupportedOperationException("Ferraloom does not implement " + type.getSimpleName()
                    + " entries yet (" + key + ")");
        };
    }

    /** The constant name an entry answers as {@code OldEnum.name()}: {@code entity.pig.ambient} is {@code ENTITY_PIG_AMBIENT}. */
    static String constantName(NamespacedKey key) {
        String name = key.getKey().toUpperCase(Locale.ROOT).replace('.', '_').replace('/', '_');
        return NamespacedKey.MINECRAFT.equals(key.getNamespace()) ? name
                : key.getNamespace().toUpperCase(Locale.ROOT) + "_" + name;
    }

    /** A registry whose entries are the keys its type's constants ask for (see the class comment). */
    private static final class Constants<T extends Keyed> implements Registry<T> {
        private static final StackWalker WALKER = StackWalker.getInstance(StackWalker.Option.RETAIN_CLASS_REFERENCE);
        private final Class<T> type;
        private final Function<NamespacedKey, T> factory;
        /** Guarded by {@code this}, in the order the constants asked. */
        private final Map<NamespacedKey, T> entries = new LinkedHashMap<>();
        /** The type has initialized: the entries are all there. */
        private volatile boolean closed;

        Constants(Class<T> type, Function<NamespacedKey, T> factory) {
            this.type = type;
            this.factory = factory;
        }

        @Override
        public T get(NamespacedKey key) {
            if (!closed) {
                if (initializingHere()) {
                    synchronized (this) {
                        return entries.computeIfAbsent(key, factory);
                    }
                }
                close();
            }
            synchronized (this) {
                return entries.get(key);
            }
        }

        @Override
        public T getOrThrow(NamespacedKey key) {
            T entry = get(key);
            if (entry == null) {
                throw new IllegalArgumentException("No " + type.getSimpleName() + " registry entry found for key " + key);
            }
            return entry;
        }

        @Override
        public Stream<T> stream() {
            return all().stream();
        }

        @Override
        public Iterator<T> iterator() {
            return all().iterator();
        }

        private List<T> all() {
            if (!closed) {
                close();
            }
            synchronized (this) {
                return List.copyOf(entries.values());
            }
        }

        /** Whether this thread is inside the type's static initializer, where its constants ask for their entries. */
        private boolean initializingHere() {
            return WALKER.walk(frames -> frames.anyMatch(f -> f.getDeclaringClass() == type
                    && f.getMethodName().equals("<clinit>")));
        }

        /**
         * Initializes the type, which asks for every constant. Never under this registry's lock: another
         * thread initializing the type holds the class's lock and needs this one.
         */
        private void close() {
            try {
                Class.forName(type.getName(), true, type.getClassLoader());
            } catch (ClassNotFoundException e) {
                throw new IllegalStateException(e);
            }
            closed = true;
        }
    }

    /**
     * An interface's entry that knows its key only, and its place: what CraftBukkit's registries answer
     * before Ferraloom implements a type. Entries are made in order under their registry's lock, so the
     * count is the ordinal.
     */
    private record KeyOnly(Class<?> type, NamespacedKey key, int ordinal) implements InvocationHandler {
        static <T> Function<NamespacedKey, T> factory(Class<T> type) {
            // With the type's nested subinterfaces ({@code BlockType.Typed}), which its constants are cast to.
            List<Class<?>> faces = new ArrayList<>(List.of(type));
            for (Class<?> nested : type.getDeclaredClasses()) {
                if (nested.isInterface() && type.isAssignableFrom(nested)) {
                    faces.add(nested);
                }
            }
            Class<?>[] implemented = faces.toArray(new Class<?>[0]);
            AtomicInteger made = new AtomicInteger();
            return key -> type.cast(Proxy.newProxyInstance(type.getClassLoader(), implemented,
                    new KeyOnly(type, key, made.getAndIncrement())));
        }

        @Override
        public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
            int arity = method.getParameterCount();
            switch (method.getName()) {
                case "getKey", "getKeyOrThrow", "getKeyOrNull" -> {
                    if (arity == 0) {
                        return key;
                    }
                }
                case "isRegistered" -> {
                    return true;
                }
                case "name" -> {
                    return constantName(key);
                }
                case "ordinal" -> {
                    return ordinal;
                }
                case "compareTo" -> {
                    if (arity == 1 && args[0] instanceof OldEnum<?> other) {
                        return Integer.compare(ordinal, other.ordinal());
                    }
                }
                case "equals" -> {
                    return proxy == args[0];
                }
                case "hashCode" -> {
                    return key.hashCode();
                }
                case "toString" -> {
                    return constantName(key);
                }
                default -> {
                }
            }
            if (method.isDefault()) {
                return InvocationHandler.invokeDefault(proxy, method, args);
            }
            throw new UnsupportedOperationException("Ferraloom does not implement " + type.getSimpleName() + "."
                    + method.getName() + "() yet");
        }
    }

    private static final class Enchantments extends StubEnchantment {
        private final NamespacedKey key;

        Enchantments(NamespacedKey key) {
            this.key = key;
        }

        @Override
        public NamespacedKey getKey() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrThrow() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrNull() {
            return key;
        }

        @Override
        public boolean isRegistered() {
            return true;
        }

        @Override
        public String getTranslationKey() {
            return "enchantment." + key.getNamespace() + "." + key.getKey();
        }
    }

    private static final class GameEvents extends StubGameEvent {
        private final NamespacedKey key;

        GameEvents(NamespacedKey key) {
            this.key = key;
        }

        @Override
        public NamespacedKey getKey() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrThrow() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrNull() {
            return key;
        }

        @Override
        public boolean isRegistered() {
            return true;
        }
    }

    private static final class Instruments extends StubMusicInstrument {
        private final NamespacedKey key;

        Instruments(NamespacedKey key) {
            this.key = key;
        }

        @Override
        public NamespacedKey getKey() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrThrow() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrNull() {
            return key;
        }

        @Override
        public boolean isRegistered() {
            return true;
        }
    }

    private static final class Effects extends StubPotionEffectType {
        private final NamespacedKey key;

        Effects(NamespacedKey key) {
            this.key = key;
        }

        @Override
        public NamespacedKey getKey() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrThrow() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrNull() {
            return key;
        }

        @Override
        public boolean isRegistered() {
            return true;
        }

        @Override
        public String getName() {
            return constantName(key);
        }

        @Override
        public String getTranslationKey() {
            return "effect." + key.getNamespace() + "." + key.getKey();
        }
    }

    private static final class Structures extends StubStructure {
        private final NamespacedKey key;

        Structures(NamespacedKey key) {
            this.key = key;
        }

        @Override
        public NamespacedKey getKey() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrThrow() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrNull() {
            return key;
        }

        @Override
        public boolean isRegistered() {
            return true;
        }
    }

    private static final class StructureTypes extends StubStructureType {
        private final NamespacedKey key;

        StructureTypes(NamespacedKey key) {
            this.key = key;
        }

        @Override
        public NamespacedKey getKey() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrThrow() {
            return key;
        }

        @Override
        public NamespacedKey getKeyOrNull() {
            return key;
        }

        @Override
        public boolean isRegistered() {
            return true;
        }
    }
}
