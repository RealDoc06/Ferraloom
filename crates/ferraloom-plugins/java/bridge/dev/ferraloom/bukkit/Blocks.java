package dev.ferraloom.bukkit;

import java.lang.reflect.Field;
import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Proxy;
import java.lang.reflect.Type;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.block.BlockType;
import org.bukkit.block.data.BlockData;

/**
 * The server's blocks, as the bridge knows them: every block's states (vanilla's global state ids),
 * its properties and a few facts, read once from the server ({@link BlockNative#table}, src/blocks.rs).
 * {@code BlockData} is a state id here: reading a block is one native call answering an int, and changing
 * a property is arithmetic, as vanilla's StateDefinition lays the states out (properties by name, the last
 * varying fastest).
 *
 * <p>Other areas use the entry points {@link #kind(Material)}, {@link #kind(int)}, {@link #data(int)},
 * {@link #parse(String)} and {@link #state(BlockData)}; they stay stable.
 */
final class Blocks {
    // Kind.flags, as src/blocks.rs writes them.
    static final int AIR = 1;
    static final int SOLID = 2;
    static final int GRAVITY = 4;
    static final int OCCLUDING = 8;
    static final int BLOCK_ENTITY = 32;
    static final int BURNABLE = 64;
    static final int REQUIRES_TOOL = 128;

    /** One block: its states are {@code base .. base + count - 1}. */
    static final class Kind {
        final int index;
        final String name;
        final NamespacedKey key;
        final int base;
        final int count;
        final int defaultState;
        final int flags;
        final float hardness;
        final float resistance;
        final float friction;
        final String item;
        final String[] props;
        final String[][] values;
        final int[] strides;
        private Material material;
        private Class<?>[] faces;

        Kind(int index, String line) {
            String[] f = line.split(" ");
            this.index = index;
            this.name = f[0];
            this.key = NamespacedKey.fromString(name);
            this.base = Integer.parseInt(f[1]);
            this.count = Integer.parseInt(f[2]);
            this.defaultState = Integer.parseInt(f[3]);
            this.flags = Integer.parseInt(f[4]);
            this.hardness = Float.parseFloat(f[5]);
            this.resistance = Float.parseFloat(f[6]);
            this.friction = Float.parseFloat(f[7]);
            this.item = f[8];
            int n = f.length - 9;
            props = new String[n];
            values = new String[n][];
            for (int i = 0; i < n; i++) {
                String p = f[9 + i];
                int eq = p.indexOf('=');
                props[i] = p.substring(0, eq);
                values[i] = p.substring(eq + 1).split(",");
            }
            strides = new int[n];
            int stride = 1;
            for (int i = n - 1; i >= 0; i--) {
                strides[i] = stride;
                stride *= values[i].length;
            }
        }

        boolean is(int flag) {
            return (flags & flag) != 0;
        }

        /** The light the block gives off (flags' bits 8 to 11). */
        int emission() {
            return flags >> 8 & 15;
        }

        /** The property's index, or -1. */
        int prop(String name) {
            for (int i = 0; i < props.length; i++) {
                if (props[i].equals(name)) {
                    return i;
                }
            }
            return -1;
        }

        /** The index of {@code state}'s value of property {@code p}. */
        int valueIndex(int state, int p) {
            return (state - base) / strides[p] % values[p].length;
        }

        String value(int state, int p) {
            return values[p][valueIndex(state, p)];
        }

        /** {@code state} with property {@code p} at value index {@code v}. */
        int with(int state, int p, int v) {
            return state + (v - valueIndex(state, p)) * strides[p];
        }

        /** {@code state} with property {@code p} set to {@code value}; IllegalArgumentException for a value it cannot take. */
        int with(int state, int p, String value) {
            for (int v = 0; v < values[p].length; v++) {
                if (values[p][v].equals(value)) {
                    return with(state, p, v);
                }
            }
            throw new IllegalArgumentException(value + " is not a value of " + props[p] + " for " + name);
        }

        /** The Material of the same name. */
        Material material() {
            if (material == null) {
                Material m = Material.matchMaterial(name);
                material = m == null ? Material.AIR : m;
            }
            return material;
        }

        /** The BlockData interface its type names ({@code BlockType.Typed<Stairs> OAK_STAIRS}), BlockData for none. */
        Class<?>[] faces() {
            if (faces == null) {
                faces = new Class<?>[] {dataClass()};
            }
            return faces;
        }

        Class<? extends BlockData> dataClass() {
            try {
                // getField does not initialize BlockType.
                Field field = BlockType.class.getField(key.getKey().toUpperCase(Locale.ROOT));
                Type type = field.getGenericType();
                if (type instanceof ParameterizedType p && p.getActualTypeArguments()[0] instanceof Class<?> c
                        && BlockData.class.isAssignableFrom(c)) {
                    return c.asSubclass(BlockData.class);
                }
            } catch (NoSuchFieldException e) {
                // A block the API does not name: plain BlockData.
            }
            return BlockData.class;
        }
    }

    private record Table(Kind[] kinds, int[] stateKind, Map<String, Kind> byName) {
    }

    private static volatile Table table;

    private Blocks() {
    }

    /** The table, read from the server on first use (on the server thread). */
    private static Table table() {
        Table t = table;
        if (t != null) {
            return t;
        }
        synchronized (Blocks.class) {
            if (table == null) {
                table = read(BlockNative.table());
            }
            return table;
        }
    }

    /** Reads the table at start, where the server lends itself (FerraloomWorld's constructor calls it). */
    static void load() {
        table();
    }

    static Table read(String text) {
        String[] lines = text.isEmpty() ? new String[0] : text.split("\n");
        Kind[] kinds = new Kind[lines.length];
        Map<String, Kind> byName = new HashMap<>();
        int states = 0;
        for (int i = 0; i < lines.length; i++) {
            kinds[i] = new Kind(i, lines[i]);
            byName.put(kinds[i].name, kinds[i]);
            states = Math.max(states, kinds[i].base + kinds[i].count);
        }
        int[] stateKind = new int[states];
        for (Kind k : kinds) {
            java.util.Arrays.fill(stateKind, k.base, k.base + k.count, k.index);
        }
        return new Table(kinds, stateKind, byName);
    }

    /** The block a state is of. */
    static Kind kind(int state) {
        Table t = table();
        if (state < 0 || state >= t.stateKind.length) {
            throw new IllegalArgumentException("No block state " + state);
        }
        return t.kinds[t.stateKind[state]];
    }

    /** The block named {@code minecraft:stone} (or {@code stone}); null for none. */
    static Kind kind(String name) {
        return table().byName.get(name.indexOf(':') < 0 ? "minecraft:" + name : name);
    }

    /** The block of a Material; null for an item that is not a block (or a legacy Material). */
    static Kind kind(Material material) {
        return material.isLegacy() ? null : kind(material.getKey().toString());
    }

    /** Every block. */
    static Kind[] kinds() {
        return table().kinds.clone();
    }

    /** A new BlockData of {@code state}. */
    static BlockData data(int state) {
        return data(state, 0);
    }

    /** A new BlockData of {@code state} whose properties in {@code parsed} (a bit per property) were given in text. */
    static BlockData data(int state, long parsed) {
        Kind k = kind(state);
        return (BlockData) Proxy.newProxyInstance(Blocks.class.getClassLoader(), k.faces(),
                new FerraloomBlockData(k, state, parsed));
    }

    /** The state id of a BlockData the bridge made; IllegalArgumentException for another's. */
    static int state(BlockData data) {
        return handler(data).state;
    }

    static FerraloomBlockData handler(BlockData data) {
        if (data != null && Proxy.isProxyClass(data.getClass())
                && Proxy.getInvocationHandler(data) instanceof FerraloomBlockData h) {
            return h;
        }
        throw new IllegalArgumentException("Not block data of this server: " + data);
    }

    /**
     * CraftBlockData.newData: {@code minecraft:oak_log[axis=x]}, {@code oak_log}, or only {@code [axis=x]}
     * for {@code material}'s block (null for none). The properties named are the parsed ones.
     */
    static BlockData parse(Material material, String text) {
        String data = text == null ? "" : text.trim();
        Kind k;
        int open = data.indexOf('[');
        String name = open < 0 ? data : data.substring(0, open);
        if (name.isEmpty()) {
            if (material == null) {
                throw new IllegalArgumentException("Could not parse data: " + text);
            }
            k = kind(material);
        } else {
            k = kind(name.toLowerCase(Locale.ROOT));
            if (k != null && material != null && kind(material) != k) {
                throw new IllegalArgumentException("Data " + text + " is not for " + material);
            }
        }
        if (k == null) {
            throw new IllegalArgumentException("Could not parse data: " + text);
        }
        int state = k.defaultState;
        long parsed = 0;
        if (open >= 0) {
            if (!data.endsWith("]")) {
                throw new IllegalArgumentException("Could not parse data: " + text);
            }
            String inner = data.substring(open + 1, data.length() - 1).trim();
            if (!inner.isEmpty()) {
                for (String pair : inner.split(",")) {
                    int eq = pair.indexOf('=');
                    int p = eq < 0 ? -1 : k.prop(pair.substring(0, eq).trim());
                    if (p < 0) {
                        throw new IllegalArgumentException("Could not parse data: " + text);
                    }
                    state = k.with(state, p, pair.substring(eq + 1).trim());
                    parsed |= 1L << p;
                }
            }
        }
        return data(state, parsed);
    }

    /** {@link #parse(Material, String)} for a full string; the state id. */
    static int parse(String text) {
        return state(parse(null, text));
    }

    /** {@code minecraft:oak_log[axis=y]}: the state's every property, or only those in {@code only}. */
    static String asString(int state, long only) {
        Kind k = kind(state);
        StringBuilder s = new StringBuilder(k.name);
        boolean first = true;
        for (int p = 0; p < k.props.length; p++) {
            if ((only & 1L << p) == 0) {
                continue;
            }
            s.append(first ? '[' : ',').append(k.props[p]).append('=').append(k.value(state, p));
            first = false;
        }
        if (!first) {
            s.append(']');
        }
        return s.toString();
    }
}
