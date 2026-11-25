package dev.ferraloom.bukkit;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import org.bukkit.ChatColor;

/**
 * Legacy text ({@code §6Shop}) to and from text components in NBT, as CraftChatMessage converts
 * item names, lore and book pages. A colour code resets the formatting, which is why a name a
 * plugin colours is not italic: {@code "§6Shop"} becomes
 * {@code {text:"",extra:[{text:"Shop",color:"gold",bold:0b,italic:0b,...}]}}.
 */
final class Texts {
    private static final String[] COLORS = {"black", "dark_blue", "dark_green", "dark_aqua", "dark_red",
        "dark_purple", "gold", "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white"};
    private static final String[] DECORATIONS = {"bold", "italic", "underlined", "strikethrough", "obfuscated"};
    private static final char[] DECORATION_CODES = {'l', 'o', 'n', 'm', 'k'};

    private Texts() {
    }

    /** Port of CraftChatMessage.fromStringOrNull (no newlines kept): null for null or empty text. */
    static Map<String, Object> fromLegacyOrNull(String legacy) {
        return legacy == null || legacy.isEmpty() ? null : fromLegacy(legacy);
    }

    /** Port of CraftChatMessage.fromString(message)[0]: the text up to the first newline. */
    static Map<String, Object> fromLegacy(String legacy) {
        return fromLegacy(legacy, false);
    }

    /** CraftChatMessage.fromString(message, true)[0], as book pages keep their lines. */
    static Map<String, Object> fromLegacyKeepingNewlines(String legacy) {
        return fromLegacy(legacy, true);
    }

    private static Map<String, Object> fromLegacy(String legacy, boolean keepNewlines) {
        List<Object> extra = new ArrayList<>();
        Map<String, Object> style = Nbt.map();
        StringBuilder hex = null;
        StringBuilder run = new StringBuilder();
        for (int i = 0; i < legacy.length(); i++) {
            char c = legacy.charAt(i);
            if (c == '\n' && !keepNewlines) {
                break;
            }
            if (c != ChatColor.COLOR_CHAR || i + 1 >= legacy.length()) {
                run.append(c);
                continue;
            }
            char code = Character.toLowerCase(legacy.charAt(i + 1));
            int color = "0123456789abcdef".indexOf(code);
            int decoration = new String(DECORATION_CODES).indexOf(code);
            if (color < 0 && decoration < 0 && code != 'r' && code != 'x') {
                run.append(c);
                continue;
            }
            i++;
            flush(extra, run, style);
            if (code == 'x') {
                hex = new StringBuilder("#");
            } else if (hex != null) {
                hex.append(code);
                if (hex.length() == 7) {
                    style = reset();
                    style.put("color", hex.toString());
                    hex = null;
                }
            } else if (decoration >= 0) {
                style = Nbt.copy(style);
                style.put(DECORATIONS[decoration], (byte) 1);
            } else {
                style = reset();
                if (color >= 0) {
                    style.put("color", COLORS[color]);
                }
            }
        }
        flush(extra, run, style);
        Map<String, Object> root = Nbt.map();
        root.put("text", "");
        if (!extra.isEmpty()) {
            root.put("extra", new Nbt.ListTag(10, extra));
        }
        return root;
    }

    /** CraftChatMessage's RESET style: every decoration explicitly off. */
    private static Map<String, Object> reset() {
        Map<String, Object> style = Nbt.map();
        for (String d : DECORATIONS) {
            style.put(d, (byte) 0);
        }
        return style;
    }

    private static void flush(List<Object> extra, StringBuilder run, Map<String, Object> style) {
        if (run.isEmpty()) {
            return;
        }
        Map<String, Object> part = Nbt.map();
        part.put("text", run.toString());
        part.putAll(style);
        extra.add(part);
        run.setLength(0);
    }

    /** Port of CraftChatMessage.fromComponent: legacy text, empty for null. */
    static String toLegacy(Object component) {
        StringBuilder out = new StringBuilder();
        boolean[] hadFormat = {false};
        visit(component, Nbt.map(), (style, text) -> {
            Object color = style.get("color");
            if (!text.isEmpty() || color != null) {
                if (color instanceof String name) {
                    int named = List.of(COLORS).indexOf(name);
                    if (named >= 0) {
                        out.append(ChatColor.COLOR_CHAR).append("0123456789abcdef".charAt(named));
                    } else if (name.startsWith("#") && name.length() == 7) {
                        out.append(ChatColor.COLOR_CHAR).append('x');
                        for (char h : name.substring(1).toLowerCase(Locale.ROOT).toCharArray()) {
                            out.append(ChatColor.COLOR_CHAR).append(h);
                        }
                    }
                    hadFormat[0] = true;
                } else if (hadFormat[0]) {
                    out.append(ChatColor.RESET);
                    hadFormat[0] = false;
                }
            }
            for (int d = 0; d < DECORATIONS.length; d++) {
                if (Nbt.intOr(style.get(DECORATIONS[d]), 0) != 0) {
                    out.append(ChatColor.COLOR_CHAR).append(DECORATION_CODES[d]);
                    hadFormat[0] = true;
                }
            }
            out.append(text);
        });
        return out.toString();
    }

    /** A component's words without formatting. */
    static String plain(Object component) {
        StringBuilder out = new StringBuilder();
        visit(component, Nbt.map(), (style, text) -> out.append(text));
        return out.toString();
    }

    /** A plain literal component. */
    static Map<String, Object> literal(String text) {
        Map<String, Object> c = Nbt.map();
        c.put("text", text);
        return c;
    }

    private interface Visitor {
        void piece(Map<String, Object> style, String text);
    }

    /** Component.visit: each piece with its style merged into its parents'. */
    private static void visit(Object component, Map<String, Object> parent, Visitor visitor) {
        switch (component) {
            case null -> {
            }
            case String s -> visitor.piece(parent, s);
            case Nbt.ListTag l -> {
                // ComponentSerialization takes a list: its first element styles the rest.
                for (Object part : l.items()) {
                    visit(part, parent, visitor);
                }
            }
            case Map<?, ?> m -> {
                Map<String, Object> style = Nbt.copy(parent);
                for (String key : new String[] {"color", "bold", "italic", "underlined", "strikethrough", "obfuscated"}) {
                    if (m.containsKey(key)) {
                        style.put(key, m.get(key));
                    }
                }
                Object text = m.get("text");
                if (text == null && m.get("") instanceof String s) {
                    text = s;
                }
                if (text == null && m.get("translate") instanceof String key) {
                    text = m.get("fallback") instanceof String f ? f : key;
                }
                visitor.piece(style, text instanceof String s ? s : "");
                for (Object child : Nbt.items(m.get("extra"))) {
                    visit(child, style, visitor);
                }
            }
            default -> visitor.piece(parent, String.valueOf(component));
        }
    }

    /** Component keys that are booleans in JSON and bytes in NBT. */
    private static final List<String> FLAGS = List.of("bold", "italic", "underlined", "strikethrough", "obfuscated", "interpret");

    /** A JSON text component (Bungee's, Gson's) as NBT. */
    static Object fromJson(JsonElement json) {
        if (json == null || json.isJsonNull()) {
            return "";
        }
        if (json.isJsonPrimitive()) {
            JsonPrimitive p = json.getAsJsonPrimitive();
            if (p.isBoolean()) {
                return Nbt.bool(p.getAsBoolean());
            }
            if (p.isNumber()) {
                Number n = p.getAsNumber();
                double d = n.doubleValue();
                return d == Math.rint(d) && Math.abs(d) < Integer.MAX_VALUE ? (Object) (int) d : (Object) d;
            }
            return p.getAsString();
        }
        if (json.isJsonArray()) {
            List<Object> items = new ArrayList<>();
            for (JsonElement e : json.getAsJsonArray()) {
                Object item = fromJson(e);
                items.add(item instanceof String s ? literal(s) : item);
            }
            return Nbt.ListTag.of(items);
        }
        Map<String, Object> m = Nbt.map();
        for (Map.Entry<String, JsonElement> e : json.getAsJsonObject().entrySet()) {
            m.put(e.getKey(), fromJson(e.getValue()));
        }
        return m;
    }

    /** An NBT text component as JSON. */
    static JsonElement toJson(Object tag) {
        return toJson(tag, null);
    }

    private static JsonElement toJson(Object tag, String key) {
        return switch (tag) {
            case String s -> new JsonPrimitive(s);
            case Byte b when key != null && FLAGS.contains(key) -> new JsonPrimitive(b != 0);
            case Number n -> new JsonPrimitive(n);
            case Nbt.ListTag l -> {
                JsonArray a = new JsonArray();
                l.items().forEach(i -> a.add(toJson(i, null)));
                yield a;
            }
            case Map<?, ?> m -> {
                JsonObject o = new JsonObject();
                m.forEach((k, v) -> o.add((String) k, toJson(v, (String) k)));
                yield o;
            }
            default -> new JsonPrimitive(String.valueOf(tag));
        };
    }
}
