package dev.ferraloom.bukkit;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Network NBT (a type byte, then the payload; no root name), as items cross JNI (src/inventory.rs).
 * Tags are plain Java values: {@code Byte}, {@code Short}, {@code Integer}, {@code Long},
 * {@code Float}, {@code Double}, {@code byte[]}, {@code String}, {@link ListTag}, a compound as a
 * {@code LinkedHashMap<String, Object>}, {@code int[]}, {@code long[]}. A value read and written
 * back unchanged keeps its exact types, so components the bridge does not model survive.
 */
final class Nbt {
    private Nbt() {
    }

    /** A list and its element type (an empty list keeps it too). */
    record ListTag(int type, List<Object> items) {
        ListTag {
            items = new ArrayList<>(items);
        }

        /** A list of {@code items}, typed by its first element (end for none). */
        static ListTag of(List<?> items) {
            return new ListTag(items.isEmpty() ? 0 : id(items.get(0)), new ArrayList<>(items));
        }
    }

    static Object read(byte[] bytes) {
        try {
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes));
            return payload(in, in.readByte(), 0);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    static byte[] write(Object tag) {
        try {
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream out = new DataOutputStream(bytes);
            out.writeByte(id(tag));
            payload(out, tag);
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    static int id(Object tag) {
        return switch (tag) {
            case Byte b -> 1;
            case Short s -> 2;
            case Integer i -> 3;
            case Long l -> 4;
            case Float f -> 5;
            case Double d -> 6;
            case byte[] a -> 7;
            case String s -> 8;
            case ListTag l -> 9;
            case Map<?, ?> m -> 10;
            case int[] a -> 11;
            case long[] a -> 12;
            default -> throw new IllegalArgumentException("not an NBT value: " + tag.getClass().getName());
        };
    }

    private static Object payload(DataInputStream in, int type, int depth) throws IOException {
        if (depth > 512) {
            throw new IOException("NBT nested too deep");
        }
        switch (type) {
            case 1:
                return in.readByte();
            case 2:
                return in.readShort();
            case 3:
                return in.readInt();
            case 4:
                return in.readLong();
            case 5:
                return in.readFloat();
            case 6:
                return in.readDouble();
            case 7: {
                byte[] a = new byte[in.readInt()];
                in.readFully(a);
                return a;
            }
            case 8:
                return in.readUTF();
            case 9: {
                int element = in.readByte();
                int n = in.readInt();
                List<Object> items = new ArrayList<>(Math.max(0, n));
                for (int i = 0; i < n; i++) {
                    items.add(payload(in, element, depth + 1));
                }
                return new ListTag(element, items);
            }
            case 10: {
                Map<String, Object> map = new LinkedHashMap<>();
                for (int t = in.readByte(); t != 0; t = in.readByte()) {
                    String name = in.readUTF();
                    map.put(name, payload(in, t, depth + 1));
                }
                return map;
            }
            case 11: {
                int[] a = new int[in.readInt()];
                for (int i = 0; i < a.length; i++) {
                    a[i] = in.readInt();
                }
                return a;
            }
            case 12: {
                long[] a = new long[in.readInt()];
                for (int i = 0; i < a.length; i++) {
                    a[i] = in.readLong();
                }
                return a;
            }
            default:
                throw new IOException("unknown NBT type " + type);
        }
    }

    private static void payload(DataOutputStream out, Object tag) throws IOException {
        switch (tag) {
            case Byte b -> out.writeByte(b);
            case Short s -> out.writeShort(s);
            case Integer i -> out.writeInt(i);
            case Long l -> out.writeLong(l);
            case Float f -> out.writeFloat(f);
            case Double d -> out.writeDouble(d);
            case byte[] a -> {
                out.writeInt(a.length);
                out.write(a);
            }
            case String s -> out.writeUTF(s);
            case ListTag l -> {
                out.writeByte(l.items().isEmpty() ? 0 : l.type());
                out.writeInt(l.items().size());
                for (Object item : l.items()) {
                    payload(out, item);
                }
            }
            case Map<?, ?> m -> {
                for (Map.Entry<?, ?> e : m.entrySet()) {
                    out.writeByte(id(e.getValue()));
                    out.writeUTF((String) e.getKey());
                    payload(out, e.getValue());
                }
                out.writeByte(0);
            }
            case int[] a -> {
                out.writeInt(a.length);
                for (int v : a) {
                    out.writeInt(v);
                }
            }
            case long[] a -> {
                out.writeInt(a.length);
                for (long v : a) {
                    out.writeLong(v);
                }
            }
            default -> throw new IllegalArgumentException("not an NBT value: " + tag.getClass().getName());
        }
    }

    /** A deep copy: compounds, lists and arrays are new, so the copy can change alone. */
    @SuppressWarnings("unchecked")
    static <T> T copy(T tag) {
        Object copy = switch (tag) {
            case byte[] a -> a.clone();
            case int[] a -> a.clone();
            case long[] a -> a.clone();
            case ListTag l -> new ListTag(l.type(), l.items().stream().map(Nbt::copy).toList());
            case Map<?, ?> m -> {
                Map<String, Object> map = new LinkedHashMap<>();
                m.forEach((k, v) -> map.put((String) k, copy(v)));
                yield map;
            }
            default -> tag;
        };
        return (T) copy;
    }

    /** Value equality, arrays by content and compounds in any order (as NBT compares them). */
    static boolean same(Object a, Object b) {
        if (a == b) {
            return true;
        }
        if (a == null || b == null) {
            return false;
        }
        return switch (a) {
            case byte[] x -> b instanceof byte[] y && Arrays.equals(x, y);
            case int[] x -> b instanceof int[] y && Arrays.equals(x, y);
            case long[] x -> b instanceof long[] y && Arrays.equals(x, y);
            case ListTag x -> b instanceof ListTag y && x.items().size() == y.items().size() && sameItems(x, y);
            case Map<?, ?> x -> b instanceof Map<?, ?> y && x.size() == y.size()
                    && x.entrySet().stream().allMatch(e -> y.containsKey(e.getKey()) && same(e.getValue(), y.get(e.getKey())));
            default -> a.equals(b);
        };
    }

    private static boolean sameItems(ListTag x, ListTag y) {
        for (int i = 0; i < x.items().size(); i++) {
            if (!same(x.items().get(i), y.items().get(i))) {
                return false;
            }
        }
        return true;
    }

    /** A hash agreeing with {@link #same}. */
    static int hash(Object tag) {
        return switch (tag) {
            case null -> 0;
            case byte[] a -> Arrays.hashCode(a);
            case int[] a -> Arrays.hashCode(a);
            case long[] a -> Arrays.hashCode(a);
            case ListTag l -> l.items().stream().mapToInt(Nbt::hash).reduce(1, (h, v) -> 31 * h + v);
            case Map<?, ?> m -> m.entrySet().stream().mapToInt(e -> e.getKey().hashCode() ^ hash(e.getValue())).sum();
            default -> tag.hashCode();
        };
    }

    /** A compound value, or null when {@code tag} is not one. */
    @SuppressWarnings("unchecked")
    static Map<String, Object> compound(Object tag) {
        return tag instanceof Map<?, ?> m ? (Map<String, Object>) m : null;
    }

    /** A number's int value, or {@code otherwise}. */
    static int intOr(Object tag, int otherwise) {
        return tag instanceof Number n ? n.intValue() : otherwise;
    }

    /** A list's items, or none when {@code tag} is not a list. */
    static List<Object> items(Object tag) {
        return tag instanceof ListTag l ? l.items() : List.of();
    }

    /** A byte holding a boolean (NBT has no boolean type). */
    static Byte bool(boolean value) {
        return value ? (byte) 1 : (byte) 0;
    }

    /** A new, empty compound. */
    static Map<String, Object> map() {
        return new LinkedHashMap<>();
    }

    /** SNBT, as {@code ItemMeta.getAsString} shows a component patch. */
    static String snbt(Object tag) {
        StringBuilder s = new StringBuilder();
        snbt(s, tag);
        return s.toString();
    }

    private static void snbt(StringBuilder s, Object tag) {
        switch (tag) {
            case Byte b -> s.append(b).append('b');
            case Short v -> s.append(v).append('s');
            case Integer v -> s.append(v);
            case Long v -> s.append(v).append('L');
            case Float v -> s.append(v).append('f');
            case Double v -> s.append(v).append('d');
            case String v -> quote(s, v);
            case byte[] a -> {
                s.append("[B;");
                for (int i = 0; i < a.length; i++) {
                    s.append(i == 0 ? "" : ",").append(a[i]).append('B');
                }
                s.append(']');
            }
            case int[] a -> {
                s.append("[I;");
                for (int i = 0; i < a.length; i++) {
                    s.append(i == 0 ? "" : ",").append(a[i]);
                }
                s.append(']');
            }
            case long[] a -> {
                s.append("[L;");
                for (int i = 0; i < a.length; i++) {
                    s.append(i == 0 ? "" : ",").append(a[i]).append('L');
                }
                s.append(']');
            }
            case ListTag l -> {
                s.append('[');
                for (int i = 0; i < l.items().size(); i++) {
                    s.append(i == 0 ? "" : ",");
                    snbt(s, l.items().get(i));
                }
                s.append(']');
            }
            case Map<?, ?> m -> {
                s.append('{');
                boolean first = true;
                for (Map.Entry<?, ?> e : m.entrySet()) {
                    s.append(first ? "" : ",");
                    first = false;
                    String key = (String) e.getKey();
                    if (key.matches("[A-Za-z0-9._+\\-]+")) {
                        s.append(key);
                    } else {
                        quote(s, key);
                    }
                    s.append(':');
                    snbt(s, e.getValue());
                }
                s.append('}');
            }
            default -> s.append(tag);
        }
    }

    private static void quote(StringBuilder s, String v) {
        s.append('"');
        for (char c : v.toCharArray()) {
            if (c == '"' || c == '\\') {
                s.append('\\');
            }
            s.append(c);
        }
        s.append('"');
    }
}
