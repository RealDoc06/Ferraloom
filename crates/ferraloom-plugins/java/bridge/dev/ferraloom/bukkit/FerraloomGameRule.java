package dev.ferraloom.bukkit;

import java.lang.reflect.Field;
import java.lang.reflect.ParameterizedType;
import java.util.Locale;
import org.bukkit.GameRule;
import org.bukkit.NamespacedKey;

/**
 * A {@link GameRule} (CraftGameRule): {@code Registry.GAME_RULE}'s entries ({@link FerraloomRegistries}).
 * Its value type is the one its API constant names ({@code GameRule<Boolean> KEEP_INVENTORY}); its default
 * is the server's.
 */
final class FerraloomGameRule<T> implements GameRule<T> {
    private final NamespacedKey key;
    private final Class<T> type;

    @SuppressWarnings("unchecked")
    FerraloomGameRule(NamespacedKey key) {
        this.key = key;
        Class<?> found = Boolean.class;
        try {
            // getField does not initialize GameRule.
            Field field = GameRule.class.getField(key.getKey().toUpperCase(Locale.ROOT));
            if (field.getGenericType() instanceof ParameterizedType p && p.getActualTypeArguments()[0] instanceof Class<?> c) {
                found = c;
            }
        } catch (NoSuchFieldException e) {
            // A rule the API does not name: read as a boolean.
        }
        this.type = (Class<T>) found;
    }

    /** The rule's value from its text, as its type reads it. */
    T parse(String text) {
        if (text == null) {
            return null;
        }
        if (type == Integer.class) {
            return type.cast(Integer.valueOf(text));
        }
        return type.cast(Boolean.valueOf(text));
    }

    @Override
    public T getDefaultValue() {
        return parse(WorldNative.gameRule(key.getKey(), true));
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
        return key.getKey();
    }

    @Override
    public Class<T> getType() {
        return type;
    }

    @Override
    public String toString() {
        return "CraftGameRule{key=" + key + "}";
    }
}
