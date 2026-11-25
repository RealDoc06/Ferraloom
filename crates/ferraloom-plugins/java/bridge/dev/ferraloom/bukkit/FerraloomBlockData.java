package dev.ferraloom.bukkit;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.EnumSet;
import java.util.Locale;
import java.util.Set;
import org.bukkit.Material;
import org.bukkit.Note;
import org.bukkit.block.BlockFace;
import org.bukkit.block.data.BlockData;

/**
 * A {@link BlockData}: one block state, as CraftBlockData is, behind a proxy implementing the interface
 * the block's type names ({@code Stairs}, {@code Ageable}...), so {@code instanceof} and casts behave.
 * Its typed accessors are vanilla's properties by name: {@code getFacing()} reads {@code facing},
 * {@code getMaximumAge()} the largest {@code age}, {@code setWaterlogged(true)} sets {@code waterlogged},
 * {@code hasFace(NORTH)} reads {@code north}. Enum values match by name ({@code BlockFace.NORTH} is
 * {@code north}), as CraftBlockData's enum properties do; {@code Bisected.Half} names vanilla's
 * {@code upper}/{@code lower} and {@code top}/{@code bottom}.
 */
final class FerraloomBlockData implements InvocationHandler {
    /** Rotatable's 16 rotations, in vanilla's {@code rotation} order (CraftRotatable). */
    private static final BlockFace[] ROTATIONS = {
        BlockFace.SOUTH, BlockFace.SOUTH_SOUTH_WEST, BlockFace.SOUTH_WEST, BlockFace.WEST_SOUTH_WEST,
        BlockFace.WEST, BlockFace.WEST_NORTH_WEST, BlockFace.NORTH_WEST, BlockFace.NORTH_NORTH_WEST,
        BlockFace.NORTH, BlockFace.NORTH_NORTH_EAST, BlockFace.NORTH_EAST, BlockFace.EAST_NORTH_EAST,
        BlockFace.EAST, BlockFace.EAST_SOUTH_EAST, BlockFace.SOUTH_EAST, BlockFace.SOUTH_SOUTH_EAST,
    };
    private static final BlockFace[] SIDES = {
        BlockFace.NORTH, BlockFace.EAST, BlockFace.SOUTH, BlockFace.WEST, BlockFace.UP, BlockFace.DOWN,
    };

    final Blocks.Kind kind;
    int state;
    /** The properties given in text (a bit per property), which {@code merge} and {@code matches} use. */
    final long parsed;

    FerraloomBlockData(Blocks.Kind kind, int state, long parsed) {
        this.kind = kind;
        this.state = state;
        this.parsed = parsed;
    }

    @Override
    public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        String name = method.getName();
        int arity = method.getParameterCount();
        switch (name) {
            case "getMaterial" -> {
                return kind.material();
            }
            case "getAsString" -> {
                boolean hide = arity == 1 && (Boolean) args[0];
                return Blocks.asString(state, hide ? parsed : -1L);
            }
            case "clone" -> {
                return Blocks.data(state, parsed);
            }
            case "equals" -> {
                return args[0] instanceof BlockData other && Proxy.isProxyClass(other.getClass())
                        && Proxy.getInvocationHandler(other) instanceof FerraloomBlockData h && h.state == state;
            }
            case "hashCode" -> {
                return state;
            }
            case "toString" -> {
                return "CraftBlockData{" + Blocks.asString(state, -1L) + "}";
            }
            case "matches" -> {
                return matches((BlockData) args[0]);
            }
            case "merge" -> {
                return merge((BlockData) args[0]);
            }
            case "copyTo" -> {
                copyTo((BlockData) args[0]);
                return null;
            }
            case "isOccluding" -> {
                return kind.is(Blocks.OCCLUDING);
            }
            case "requiresCorrectToolForDrops" -> {
                return kind.is(Blocks.REQUIRES_TOOL);
            }
            case "getLightEmission" -> {
                return kind.emission();
            }
            case "getPlacementMaterial" -> {
                Material item = Material.matchMaterial(kind.item);
                return item == null ? Material.AIR : item;
            }
            case "createBlockState" -> {
                return new FerraloomBlockState(null, 0, 0, 0, state);
            }
            default -> {
                // A property accessor, below.
            }
        }
        Object answer = property(method, args);
        if (answer != NONE) {
            return answer;
        }
        if (method.isDefault()) {
            return InvocationHandler.invokeDefault(proxy, method, args);
        }
        throw new UnsupportedOperationException("Ferraloom does not implement "
                + method.getDeclaringClass().getSimpleName() + "." + name + "() yet");
    }

    private static final Object NONE = new Object();

    /** The property accessors; {@link #NONE} when the method is not one. */
    private Object property(Method method, Object[] args) {
        String name = method.getName();
        int arity = method.getParameterCount();
        Class<?> ret = method.getReturnType();
        if (arity == 0) {
            switch (name) {
                case "getRotation" -> {
                    int p = kind.prop("rotation");
                    if (p >= 0) {
                        return ROTATIONS[Integer.parseInt(kind.value(state, p))];
                    }
                }
                case "getFaces" -> {
                    int facing = kind.prop("facing");
                    return facing >= 0 ? values(facing) : faces(true);
                }
                case "getAllowedFaces" -> {
                    return faces(false);
                }
                case "getAttachedFace" -> {
                    return get("face", ret);
                }
                case "getNote" -> {
                    int p = kind.prop("note");
                    if (p >= 0) {
                        return new Note(Integer.parseInt(kind.value(state, p)));
                    }
                }
                default -> {
                }
            }
            if (name.startsWith("getMaximum") || name.startsWith("getMinimum")) {
                int p = kind.prop(snake(name.substring(10)));
                if (p < 0) {
                    return NONE;
                }
                int best = Integer.parseInt(kind.values[p][0]);
                for (String v : kind.values[p]) {
                    int i = Integer.parseInt(v);
                    best = name.startsWith("getMaximum") ? Math.max(best, i) : Math.min(best, i);
                }
                return best;
            }
            String prop = accessor(name);
            return prop == null ? NONE : get(prop, ret);
        }
        if (arity == 1 && args[0] instanceof BlockFace face && !name.startsWith("set")) {
            // MultipleFacing.hasFace, RedstoneWire.getFace, Wall.getHeight: the side's own property.
            return get(face.name().toLowerCase(Locale.ROOT), ret);
        }
        if (arity == 1 && name.startsWith("set")) {
            Object value = args[0];
            switch (name) {
                case "setRotation" -> {
                    int p = kind.prop("rotation");
                    for (int i = 0; p >= 0 && i < ROTATIONS.length; i++) {
                        if (ROTATIONS[i] == value) {
                            state = kind.with(state, p, String.valueOf(i));
                            return null;
                        }
                    }
                    if (p >= 0) {
                        throw new IllegalArgumentException("Illegal rotation " + value);
                    }
                }
                case "setAttachedFace" -> {
                    set("face", value);
                    return null;
                }
                case "setNote" -> {
                    if (value instanceof Note note) {
                        set("note", note.getId());
                        return null;
                    }
                }
                default -> {
                }
            }
            String prop = snake(name.substring(3));
            if (kind.prop(prop) < 0) {
                return NONE;
            }
            set(prop, value);
            return null;
        }
        if (arity == 2 && name.startsWith("set") && args[0] instanceof BlockFace face) {
            // MultipleFacing.setFace, RedstoneWire.setFace, Wall.setHeight.
            set(face.name().toLowerCase(Locale.ROOT), args[1]);
            return null;
        }
        return NONE;
    }

    /** {@code getHoneyLevel}, {@code isWaterlogged}, {@code hasBottle_0}: the property it reads, or null. */
    private String accessor(String name) {
        for (String prefix : new String[] {"get", "is", "has"}) {
            if (name.startsWith(prefix) && name.length() > prefix.length()) {
                String prop = snake(name.substring(prefix.length()));
                return kind.prop(prop) >= 0 ? prop : null;
            }
        }
        return null;
    }

    /** {@code HoneyLevel} → {@code honey_level}. */
    static String snake(String camel) {
        StringBuilder s = new StringBuilder();
        for (int i = 0; i < camel.length(); i++) {
            char c = camel.charAt(i);
            if (Character.isUpperCase(c)) {
                if (i > 0) {
                    s.append('_');
                }
                s.append(Character.toLowerCase(c));
            } else {
                s.append(c);
            }
        }
        return s.toString();
    }

    private Object get(String prop, Class<?> type) {
        int p = kind.prop(prop);
        if (p < 0) {
            throw new IllegalArgumentException(kind.name + " has no property " + prop);
        }
        String value = kind.value(state, p);
        if (type == int.class) {
            return Integer.parseInt(value);
        }
        if (type == boolean.class) {
            return "true".equals(value);
        }
        if (type.isEnum()) {
            return constant(type, value);
        }
        return value;
    }

    private void set(String prop, Object value) {
        int p = kind.prop(prop);
        if (p < 0) {
            throw new IllegalArgumentException(kind.name + " has no property " + prop);
        }
        String text = value instanceof Enum<?> e ? vanillaName(e, kind.values[p]) : String.valueOf(value);
        state = kind.with(state, p, text);
    }

    /** The enum constant vanilla's value names: by name, and Bisected.Half's TOP/BOTTOM for upper/lower. */
    private static Object constant(Class<?> type, String value) {
        String wanted = switch (value) {
            case "upper" -> "TOP";
            case "lower" -> "BOTTOM";
            default -> value.toUpperCase(Locale.ROOT);
        };
        for (Object c : type.getEnumConstants()) {
            if (((Enum<?>) c).name().equals(wanted)) {
                return c;
            }
        }
        throw new IllegalStateException("No " + type.getSimpleName() + " for " + value);
    }

    /** The property value an enum constant stands for, among those the property has. */
    private static String vanillaName(Enum<?> e, String[] values) {
        String lower = e.name().toLowerCase(Locale.ROOT);
        for (String v : values) {
            if (v.equals(lower) || (lower.equals("top") && v.equals("upper"))
                    || (lower.equals("bottom") && v.equals("lower"))) {
                return v;
            }
        }
        return lower;
    }

    /** The values of an enum-valued property as BlockFaces (Directional.getFaces). */
    private Set<BlockFace> values(int p) {
        Set<BlockFace> out = EnumSet.noneOf(BlockFace.class);
        for (String v : kind.values[p]) {
            out.add(BlockFace.valueOf(v.toUpperCase(Locale.ROOT)));
        }
        return out;
    }

    /** The sides with a property (MultipleFacing.getAllowedFaces), or only those set true (getFaces). */
    private Set<BlockFace> faces(boolean set) {
        Set<BlockFace> out = EnumSet.noneOf(BlockFace.class);
        for (BlockFace face : SIDES) {
            int p = kind.prop(face.name().toLowerCase(Locale.ROOT));
            if (p >= 0 && (!set || "true".equals(kind.value(state, p)))) {
                out.add(face);
            }
        }
        return out;
    }

    /** CraftBlockData.matches: the same block, and the same values of the properties {@code data} was parsed with. */
    private boolean matches(BlockData data) {
        if (data == null) {
            return false;
        }
        FerraloomBlockData other = Blocks.handler(data);
        if (other.kind != kind) {
            return false;
        }
        return other.state == state || (other.parsed != 0 && Blocks.state(merge(data)) == state);
    }

    /** CraftBlockData.merge: this state with {@code data}'s parsed properties. */
    private BlockData merge(BlockData data) {
        FerraloomBlockData other = Blocks.handler(data);
        if (other.kind != kind) {
            throw new IllegalArgumentException("States have different types (got " + data + ", expected " + this + ")");
        }
        if (other.parsed == 0) {
            throw new IllegalArgumentException("Data not created via string parsing");
        }
        int merged = state;
        for (int p = 0; p < kind.props.length; p++) {
            if ((other.parsed & 1L << p) != 0) {
                merged = kind.with(merged, p, other.kind.valueIndex(other.state, p));
            }
        }
        return Blocks.data(merged, parsed);
    }

    /** CraftBlockData.copyTo: this state's values of the properties {@code data}'s block has too. */
    private void copyTo(BlockData data) {
        FerraloomBlockData other = Blocks.handler(data);
        for (int p = 0; p < kind.props.length; p++) {
            int q = other.kind.prop(kind.props[p]);
            if (q >= 0) {
                other.state = other.kind.with(other.state, q, kind.value(state, p));
            }
        }
    }

    @Override
    public String toString() {
        return Blocks.asString(state, -1L);
    }
}
