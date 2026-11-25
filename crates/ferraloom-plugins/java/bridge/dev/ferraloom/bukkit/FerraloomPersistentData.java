package dev.ferraloom.bukkit;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.ListPersistentDataType;
import org.bukkit.persistence.PersistentDataAdapterContext;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/**
 * CraftPersistentDataContainer: plugin data as NBT, by {@code namespace:key}. An item keeps it in
 * {@code minecraft:custom_data} under {@code PublicBukkitValues}, as CraftBukkit does, so items
 * carry their plugin data to and from CraftBukkit servers' worlds. The primitive types map to
 * NBT as CraftPersistentDataTypeRegistry maps them.
 */
final class FerraloomPersistentData implements PersistentDataContainer {
    /** CraftMetaItem.BUKKIT_CUSTOM_TAG: the key in {@code custom_data}. */
    static final String BUKKIT = "PublicBukkitValues";
    private static final PersistentDataAdapterContext CONTEXT = () -> new FerraloomPersistentData(Nbt.map());

    final Map<String, Object> tags;

    FerraloomPersistentData(Map<String, Object> tags) {
        this.tags = tags;
    }

    @Override
    public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        if (key == null || type == null || value == null) {
            throw new IllegalArgumentException("The key, type and value must not be null");
        }
        tags.put(key.toString(), wrap(type.toPrimitive(value, CONTEXT), type));
    }

    @Override
    public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        Object tag = tags.get(key.toString());
        return tag != null && matches(tag, type);
    }

    @Override
    public boolean has(NamespacedKey key) {
        return tags.containsKey(key.toString());
    }

    @Override
    public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        Object tag = tags.get(key.toString());
        if (tag == null) {
            return null;
        }
        if (!matches(tag, type)) {
            throw new IllegalArgumentException("The found tag instance (" + tag.getClass().getSimpleName()
                    + ") cannot store " + type.getPrimitiveType().getSimpleName());
        }
        return type.fromPrimitive(type.getPrimitiveType().cast(unwrap(tag, type)), CONTEXT);
    }

    @Override
    public <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C otherwise) {
        C value = get(key, type);
        return value == null ? otherwise : value;
    }

    @Override
    public Set<NamespacedKey> getKeys() {
        Set<NamespacedKey> keys = new LinkedHashSet<>();
        for (String k : tags.keySet()) {
            NamespacedKey key = NamespacedKey.fromString(k);
            if (key != null) {
                keys.add(key);
            }
        }
        return keys;
    }

    @Override
    public void remove(NamespacedKey key) {
        tags.remove(key.toString());
    }

    @Override
    public boolean isEmpty() {
        return tags.isEmpty();
    }

    @Override
    public void copyTo(PersistentDataContainer other, boolean replace) {
        if (!(other instanceof FerraloomPersistentData to)) {
            throw new IllegalArgumentException("not a Ferraloom container");
        }
        tags.forEach((k, v) -> {
            if (replace || !to.tags.containsKey(k)) {
                to.tags.put(k, Nbt.copy(v));
            }
        });
    }

    @Override
    public PersistentDataAdapterContext getAdapterContext() {
        return CONTEXT;
    }

    /** A primitive as NBT: numbers and strings as themselves, containers as compounds, lists as lists. */
    private static Object wrap(Object primitive, PersistentDataType<?, ?> type) {
        return switch (primitive) {
            case FerraloomPersistentData c -> Nbt.copy(c.tags);
            case PersistentDataContainer[] cs -> {
                List<Object> list = new ArrayList<>();
                for (PersistentDataContainer c : cs) {
                    list.add(Nbt.copy(((FerraloomPersistentData) c).tags));
                }
                yield new Nbt.ListTag(10, list);
            }
            case List<?> l -> {
                PersistentDataType<?, ?> element = type instanceof ListPersistentDataType<?, ?> lt ? lt.elementType() : null;
                List<Object> list = new ArrayList<>();
                for (Object item : l) {
                    list.add(wrap(item, element));
                }
                yield Nbt.ListTag.of(list);
            }
            case Boolean b -> Nbt.bool(b);
            default -> {
                Nbt.id(primitive);
                yield primitive;
            }
        };
    }

    private static Object unwrap(Object tag, PersistentDataType<?, ?> type) {
        Class<?> primitive = type.getPrimitiveType();
        if (primitive == PersistentDataContainer.class) {
            return new FerraloomPersistentData(Nbt.copy(Nbt.compound(tag)));
        }
        if (primitive == PersistentDataContainer[].class) {
            return Nbt.items(tag).stream().map(t -> new FerraloomPersistentData(Nbt.copy(Nbt.compound(t))))
                    .toArray(PersistentDataContainer[]::new);
        }
        if (primitive == List.class && type instanceof ListPersistentDataType<?, ?> lt) {
            List<Object> out = new ArrayList<>();
            for (Object item : Nbt.items(tag)) {
                out.add(lt.elementType().getPrimitiveType().cast(unwrap(item, lt.elementType())));
            }
            return out;
        }
        if (primitive == Boolean.class) {
            return Nbt.intOr(tag, 0) != 0;
        }
        return Nbt.copy(tag);
    }

    private static boolean matches(Object tag, PersistentDataType<?, ?> type) {
        Class<?> primitive = type.getPrimitiveType();
        if (primitive == PersistentDataContainer.class) {
            return tag instanceof Map;
        }
        if (primitive == PersistentDataContainer[].class) {
            return tag instanceof Nbt.ListTag l && (l.items().isEmpty() || l.type() == 10);
        }
        if (primitive == List.class) {
            return tag instanceof Nbt.ListTag;
        }
        if (primitive == Boolean.class) {
            return tag instanceof Byte;
        }
        return primitive.isInstance(tag);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomPersistentData p && Nbt.same(tags, p.tags);
    }

    @Override
    public int hashCode() {
        return Nbt.hash(tags);
    }
}
