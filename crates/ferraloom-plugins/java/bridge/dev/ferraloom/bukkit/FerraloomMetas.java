package dev.ferraloom.bukkit;

import com.google.common.collect.ImmutableList;
import com.google.common.collect.ImmutableMap;
import java.net.URI;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.md_5.bungee.api.chat.BaseComponent;
import net.md_5.bungee.chat.ComponentSerializer;
import org.bukkit.Bukkit;
import org.bukkit.Color;
import org.bukkit.NamespacedKey;
import org.bukkit.OfflinePlayer;
import org.bukkit.Registry;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.inventory.meta.EnchantmentStorageMeta;
import org.bukkit.inventory.meta.LeatherArmorMeta;
import org.bukkit.inventory.meta.PotionMeta;
import org.bukkit.inventory.meta.SkullMeta;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionEffectType;
import org.bukkit.potion.PotionType;
import org.bukkit.profile.PlayerProfile;

/** The typed metas a material asks for, each over its components (CraftMetaSkull, CraftMetaPotion...). */
final class FerraloomMetas {
    private FerraloomMetas() {
    }

    /** CraftMetaSkull: {@code minecraft:profile} ({@code {name, id, properties}}) and {@code minecraft:note_block_sound}. */
    static final class Skull extends FerraloomItemMeta implements SkullMeta {
        Skull(Map<String, Object> components) {
            super(components);
        }

        @Override
        String metaType() {
            return "SKULL";
        }

        private Map<String, Object> profile() {
            Object p = get("profile");
            if (p instanceof String name) {
                Map<String, Object> m = Nbt.map();
                m.put("name", name);
                return m;
            }
            return Nbt.compound(p);
        }

        private UUID id() {
            Map<String, Object> p = profile();
            if (p != null && p.get("id") instanceof int[] a && a.length == 4) {
                return new UUID((long) a[0] << 32 | a[1] & 0xFFFFFFFFL, (long) a[2] << 32 | a[3] & 0xFFFFFFFFL);
            }
            return null;
        }

        private static int[] ints(UUID id) {
            long m = id.getMostSignificantBits();
            long l = id.getLeastSignificantBits();
            return new int[] {(int) (m >> 32), (int) m, (int) (l >> 32), (int) l};
        }

        @Override
        public String getOwner() {
            Map<String, Object> p = profile();
            return p != null && p.get("name") instanceof String name ? name : null;
        }

        @Override
        public boolean hasOwner() {
            Map<String, Object> p = profile();
            return p != null && (p.get("name") instanceof String || p.get("id") != null);
        }

        @Override
        public boolean setOwner(String name) {
            if (name != null && name.length() > 16) {
                return false;
            }
            Map<String, Object> p = null;
            if (name != null) {
                p = Nbt.map();
                p.put("name", name);
            }
            put("profile", p);
            return true;
        }

        @Override
        public OfflinePlayer getOwningPlayer() {
            UUID id = id();
            if (id != null) {
                return Bukkit.getOfflinePlayer(id);
            }
            String name = getOwner();
            return name == null ? null : Bukkit.getOfflinePlayer(name);
        }

        @Override
        public boolean setOwningPlayer(OfflinePlayer owner) {
            if (owner == null) {
                put("profile", null);
                return true;
            }
            Map<String, Object> p = Nbt.map();
            if (owner.getName() != null) {
                p.put("name", owner.getName());
            }
            p.put("id", ints(owner.getUniqueId()));
            put("profile", p);
            return true;
        }

        /** The skin URL a {@code textures} property names, or null. */
        private URL skin() {
            Map<String, Object> p = profile();
            for (Object prop : Nbt.items(p == null ? null : p.get("properties"))) {
                Map<String, Object> m = Nbt.compound(prop);
                if (m != null && "textures".equals(m.get("name")) && m.get("value") instanceof String value) {
                    String json = new String(Base64.getDecoder().decode(value), StandardCharsets.UTF_8);
                    var root = com.google.gson.JsonParser.parseString(json).getAsJsonObject();
                    var skin = root.getAsJsonObject("textures").getAsJsonObject("SKIN");
                    try {
                        return skin == null ? null : URI.create(skin.get("url").getAsString()).toURL();
                    } catch (Exception e) {
                        return null;
                    }
                }
            }
            return null;
        }

        @Override
        public PlayerProfile getOwnerProfile() {
            if (!hasOwner()) {
                return null;
            }
            PlayerProfile profile = Bukkit.createPlayerProfile(id(), getOwner());
            URL skin = skin();
            if (skin != null) {
                profile.getTextures().setSkin(skin);
            }
            return profile;
        }

        /** The profile's id, name and skin, the skin as the {@code textures} property heads carry. */
        @Override
        public void setOwnerProfile(PlayerProfile profile) {
            if (profile == null) {
                put("profile", null);
                return;
            }
            Map<String, Object> p = Nbt.map();
            if (profile.getName() != null) {
                p.put("name", profile.getName());
            }
            if (profile.getUniqueId() != null) {
                p.put("id", ints(profile.getUniqueId()));
            }
            URL skin = profile.getTextures().getSkin();
            if (skin != null) {
                String json = "{\"textures\":{\"SKIN\":{\"url\":\"" + skin + "\"}}}";
                Map<String, Object> textures = Nbt.map();
                textures.put("name", "textures");
                textures.put("value", Base64.getEncoder().encodeToString(json.getBytes(StandardCharsets.UTF_8)));
                p.put("properties", new Nbt.ListTag(10, List.of(textures)));
            }
            put("profile", p);
        }

        @Override
        public void setNoteBlockSound(NamespacedKey sound) {
            put("note_block_sound", sound == null ? null : sound.toString());
        }

        @Override
        public NamespacedKey getNoteBlockSound() {
            return get("note_block_sound") instanceof String s ? NamespacedKey.fromString(s) : null;
        }

        @Override
        public Skull clone() {
            return (Skull) copy();
        }
    }

    /** CraftMetaPotion: {@code minecraft:potion_contents} and {@code minecraft:potion_duration_scale}. */
    static final class Potion extends FerraloomItemMeta implements PotionMeta {
        Potion(Map<String, Object> components) {
            super(components);
        }

        @Override
        String metaType() {
            return "POTION";
        }

        private Map<String, Object> contents() {
            Object c = get("potion_contents");
            if (c instanceof String potion) {
                Map<String, Object> m = Nbt.map();
                m.put("potion", potion);
                return m;
            }
            Map<String, Object> m = Nbt.compound(c);
            return m == null ? Nbt.map() : Nbt.copy(m);
        }

        private void setContents(Map<String, Object> c) {
            put("potion_contents", c.isEmpty() ? null : c);
        }

        @Override
        public void setBasePotionType(PotionType type) {
            Map<String, Object> c = contents();
            if (type == null) {
                c.remove("potion");
            } else {
                c.put("potion", type.getKey().toString());
            }
            setContents(c);
        }

        @Override
        public PotionType getBasePotionType() {
            if (!(contents().get("potion") instanceof String id)) {
                return null;
            }
            NamespacedKey key = NamespacedKey.fromString(id);
            for (PotionType type : PotionType.values()) {
                if (type.getKey().equals(key)) {
                    return type;
                }
            }
            return null;
        }

        @Override
        public boolean hasBasePotionType() {
            return getBasePotionType() != null;
        }

        @SuppressWarnings({"deprecation", "removal"})
        @Override
        public void setBasePotionData(org.bukkit.potion.PotionData data) {
            setBasePotionType(data == null ? null : data.getType());
        }

        @SuppressWarnings({"deprecation", "removal"})
        @Override
        public org.bukkit.potion.PotionData getBasePotionData() {
            PotionType type = getBasePotionType();
            return type == null ? null : new org.bukkit.potion.PotionData(type);
        }

        private static PotionEffect effect(Map<String, Object> m) {
            if (m == null || !(m.get("id") instanceof String id)) {
                return null;
            }
            NamespacedKey key = NamespacedKey.fromString(id);
            PotionEffectType type = key == null ? null : Registry.EFFECT.get(key);
            if (type == null) {
                return null;
            }
            return new PotionEffect(type, Nbt.intOr(m.get("duration"), 0), Nbt.intOr(m.get("amplifier"), 0),
                    Nbt.intOr(m.get("ambient"), 0) != 0, Nbt.intOr(m.get("show_particles"), 1) != 0,
                    Nbt.intOr(m.get("show_icon"), 1) != 0);
        }

        private static Map<String, Object> effectTag(PotionEffect e) {
            Map<String, Object> m = Nbt.map();
            m.put("id", e.getType().getKey().toString());
            m.put("amplifier", (byte) e.getAmplifier());
            m.put("duration", e.getDuration());
            m.put("ambient", Nbt.bool(e.isAmbient()));
            m.put("show_particles", Nbt.bool(e.hasParticles()));
            m.put("show_icon", Nbt.bool(e.hasIcon()));
            return m;
        }

        private List<PotionEffect> effects() {
            List<PotionEffect> out = new ArrayList<>();
            for (Object t : Nbt.items(contents().get("custom_effects"))) {
                PotionEffect e = effect(Nbt.compound(t));
                if (e != null) {
                    out.add(e);
                }
            }
            return out;
        }

        private void setEffects(List<PotionEffect> effects) {
            Map<String, Object> c = contents();
            if (effects.isEmpty()) {
                c.remove("custom_effects");
            } else {
                c.put("custom_effects", new Nbt.ListTag(10, effects.stream().map(e -> (Object) effectTag(e)).toList()));
            }
            setContents(c);
        }

        @Override
        public boolean hasCustomEffects() {
            return !Nbt.items(contents().get("custom_effects")).isEmpty();
        }

        @Override
        public List<PotionEffect> getCustomEffects() {
            return ImmutableList.copyOf(effects());
        }

        @Override
        public boolean addCustomEffect(PotionEffect effect, boolean overwrite) {
            List<PotionEffect> effects = effects();
            for (int i = 0; i < effects.size(); i++) {
                if (effects.get(i).getType().equals(effect.getType())) {
                    if (!overwrite) {
                        return false;
                    }
                    effects.set(i, effect);
                    setEffects(effects);
                    return true;
                }
            }
            effects.add(effect);
            setEffects(effects);
            return true;
        }

        @Override
        public boolean removeCustomEffect(PotionEffectType type) {
            List<PotionEffect> effects = effects();
            boolean removed = effects.removeIf(e -> e.getType().equals(type));
            setEffects(effects);
            return removed;
        }

        @Override
        public boolean hasCustomEffect(PotionEffectType type) {
            return effects().stream().anyMatch(e -> e.getType().equals(type));
        }

        @Override
        public boolean setMainEffect(PotionEffectType type) {
            List<PotionEffect> effects = effects();
            for (int i = 0; i < effects.size(); i++) {
                if (effects.get(i).getType().equals(type)) {
                    effects.add(0, effects.remove(i));
                    setEffects(effects);
                    return true;
                }
            }
            return false;
        }

        @Override
        public boolean clearCustomEffects() {
            boolean had = hasCustomEffects();
            setEffects(List.of());
            return had;
        }

        @Override
        public boolean hasColor() {
            return contents().containsKey("custom_color");
        }

        @Override
        public Color getColor() {
            Object c = contents().get("custom_color");
            return c == null ? null : Color.fromRGB(Nbt.intOr(c, 0) & 0xFFFFFF);
        }

        @Override
        public void setColor(Color color) {
            Map<String, Object> c = contents();
            if (color == null) {
                c.remove("custom_color");
            } else {
                c.put("custom_color", color.asRGB());
            }
            setContents(c);
        }

        @Override
        public boolean hasCustomName() {
            return contents().containsKey("custom_name");
        }

        @Override
        public String getCustomName() {
            return contents().get("custom_name") instanceof String s ? s : null;
        }

        @Override
        public void setCustomName(String name) {
            Map<String, Object> c = contents();
            if (name == null) {
                c.remove("custom_name");
            } else {
                c.put("custom_name", name);
            }
            setContents(c);
        }

        @Override
        public boolean hasDurationScale() {
            return has("potion_duration_scale");
        }

        @Override
        public float getDurationScale() {
            return get("potion_duration_scale") instanceof Number n ? n.floatValue() : 1;
        }

        @Override
        public void setDurationScale(Float scale) {
            put("potion_duration_scale", scale);
        }

        @Override
        public Potion clone() {
            return (Potion) copy();
        }
    }

    /** CraftMetaLeatherArmor: {@code minecraft:dyed_color}. */
    static final class Leather extends FerraloomItemMeta implements LeatherArmorMeta {
        /** CraftItemFactory.DEFAULT_LEATHER_COLOR. */
        static final Color DEFAULT = Color.fromRGB(0xA06540);

        Leather(Map<String, Object> components) {
            super(components);
        }

        @Override
        String metaType() {
            return "LEATHER_ARMOR";
        }

        @Override
        public Color getColor() {
            Object rgb = get("dyed_color");
            if (rgb instanceof Map<?, ?> m) {
                rgb = m.get("rgb");
            }
            return rgb == null ? DEFAULT : Color.fromRGB(Nbt.intOr(rgb, 0) & 0xFFFFFF);
        }

        @Override
        public void setColor(Color color) {
            put("dyed_color", color == null ? null : (Object) color.asRGB());
        }

        @Override
        public Leather clone() {
            return (Leather) copy();
        }
    }

    /** CraftMetaEnchantedBook: {@code minecraft:stored_enchantments}. */
    static final class EnchantedBook extends FerraloomItemMeta implements EnchantmentStorageMeta {
        EnchantedBook(Map<String, Object> components) {
            super(components);
        }

        @Override
        String metaType() {
            return "ENCHANTED";
        }

        @Override
        public boolean hasStoredEnchants() {
            return !enchants(compound("stored_enchantments")).isEmpty();
        }

        @Override
        public boolean hasStoredEnchant(Enchantment e) {
            Map<String, Object> levels = compound("stored_enchantments");
            return levels != null && levels.containsKey(e.getKey().toString());
        }

        @Override
        public int getStoredEnchantLevel(Enchantment e) {
            Map<String, Object> levels = compound("stored_enchantments");
            return levels == null ? 0 : Nbt.intOr(levels.get(e.getKey().toString()), 0);
        }

        @Override
        public Map<Enchantment, Integer> getStoredEnchants() {
            return ImmutableMap.copyOf(enchants(compound("stored_enchantments")));
        }

        @Override
        public boolean addStoredEnchant(Enchantment e, int level, boolean ignoreLevelRestriction) {
            return addEnchant("stored_enchantments", e, level, ignoreLevelRestriction);
        }

        @Override
        public boolean removeStoredEnchant(Enchantment e) {
            return removeEnchant("stored_enchantments", e);
        }

        @Override
        public boolean hasConflictingStoredEnchant(Enchantment e) {
            return conflicts("stored_enchantments", e);
        }

        @Override
        public EnchantedBook clone() {
            return (EnchantedBook) copy();
        }
    }

    /**
     * CraftMetaBook and CraftMetaBookSigned: {@code minecraft:writable_book_content}'s raw pages or
     * {@code minecraft:written_book_content}'s title, author, generation and component pages.
     */
    static final class Book extends FerraloomItemMeta implements BookMeta {
        static final int MAX_PAGES = 100;
        static final int MAX_TITLE = 32;
        final boolean signed;
        private final BookMeta.Spigot spigot = new BookMeta.Spigot() {
            @Override
            public BaseComponent[] getPage(int page) {
                return ComponentSerializer.parse(Texts.toJson(pageTags().get(page - 1)).toString());
            }

            @Override
            public void setPage(int page, BaseComponent... text) {
                List<Object> pages = pageTags();
                pages.set(page - 1, component(text));
                setPageTags(pages);
            }

            @Override
            public List<BaseComponent[]> getPages() {
                List<BaseComponent[]> out = new ArrayList<>();
                for (Object page : pageTags()) {
                    out.add(ComponentSerializer.parse(Texts.toJson(page).toString()));
                }
                return out;
            }

            @Override
            public void setPages(List<BaseComponent[]> pages) {
                setPageTags(pages.stream().map(p -> component(p)).toList());
            }

            @Override
            public void setPages(BaseComponent[]... pages) {
                setPages(Arrays.asList(pages));
            }

            @Override
            public void addPage(BaseComponent[]... pages) {
                List<Object> tags = pageTags();
                for (BaseComponent[] page : pages) {
                    tags.add(component(page));
                }
                setPageTags(tags);
            }
        };

        Book(Map<String, Object> components, boolean signed) {
            super(components);
            this.signed = signed;
        }

        @Override
        String metaType() {
            return signed ? "BOOK_SIGNED" : "BOOK";
        }

        private String id() {
            return signed ? "written_book_content" : "writable_book_content";
        }

        private Map<String, Object> content() {
            Map<String, Object> c = compound(id());
            return c == null ? Nbt.map() : Nbt.copy(c);
        }

        private void setContent(Map<String, Object> c) {
            put(id(), c.isEmpty() || (!signed && Nbt.items(c.get("pages")).isEmpty()) ? null : c);
        }

        /** A Filterable value: its {@code raw} form. */
        private static Object raw(Object filterable) {
            Map<String, Object> m = Nbt.compound(filterable);
            return m != null && m.containsKey("raw") ? m.get("raw") : filterable;
        }

        private static Map<String, Object> filterable(Object value) {
            Map<String, Object> m = Nbt.map();
            m.put("raw", value);
            return m;
        }

        /** The pages as components (a writable book's raw text as literals). */
        List<Object> pageTags() {
            List<Object> out = new ArrayList<>();
            for (Object page : Nbt.items(content().get("pages"))) {
                Object raw = raw(page);
                out.add(signed ? raw : Texts.literal(raw instanceof String s ? s : ""));
            }
            return out;
        }

        void setPageTags(List<Object> pages) {
            Map<String, Object> c = content();
            List<Object> list = new ArrayList<>();
            for (Object page : pages.subList(0, Math.min(pages.size(), MAX_PAGES))) {
                list.add(filterable(signed ? page : Texts.plain(page)));
            }
            if (list.isEmpty()) {
                c.remove("pages");
            } else {
                c.put("pages", new Nbt.ListTag(10, list));
            }
            if (signed) {
                c.putIfAbsent("title", filterable(""));
                c.putIfAbsent("author", "");
                c.putIfAbsent("resolved", Nbt.bool(true));
            }
            setContent(c);
        }

        private static Object component(BaseComponent... text) {
            return Texts.fromJson(com.google.gson.JsonParser.parseString(ComponentSerializer.toString(text)));
        }

        private Object page(String text) {
            return signed ? Texts.fromLegacyKeepingNewlines(text == null ? "" : text) : Texts.literal(text == null ? "" : text);
        }

        @Override
        public boolean hasPages() {
            return !Nbt.items(content().get("pages")).isEmpty();
        }

        @Override
        public String getPage(int page) {
            List<Object> pages = pageTags();
            if (page < 1 || page > pages.size()) {
                throw new IllegalArgumentException("Invalid page number " + page + "/" + pages.size());
            }
            return signed ? Texts.toLegacy(pages.get(page - 1)) : Texts.plain(pages.get(page - 1));
        }

        @Override
        public void setPage(int page, String text) {
            List<Object> pages = pageTags();
            if (page < 1 || page > pages.size()) {
                throw new IllegalArgumentException("Invalid page number " + page + "/" + pages.size());
            }
            pages.set(page - 1, page(text));
            setPageTags(pages);
        }

        @Override
        public List<String> getPages() {
            List<String> out = new ArrayList<>();
            for (Object page : pageTags()) {
                out.add(signed ? Texts.toLegacy(page) : Texts.plain(page));
            }
            return ImmutableList.copyOf(out);
        }

        @Override
        public void setPages(List<String> pages) {
            setPageTags(pages.stream().map(this::page).toList());
        }

        @Override
        public void setPages(String... pages) {
            setPages(Arrays.asList(pages));
        }

        @Override
        public void addPage(String... pages) {
            List<Object> tags = pageTags();
            for (String page : pages) {
                tags.add(page(page));
            }
            setPageTags(tags);
        }

        @Override
        public int getPageCount() {
            return Nbt.items(content().get("pages")).size();
        }

        @Override
        public boolean hasTitle() {
            return signed && raw(content().get("title")) instanceof String s && !s.isEmpty();
        }

        @Override
        public String getTitle() {
            return hasTitle() ? (String) raw(content().get("title")) : null;
        }

        @Override
        public boolean setTitle(String title) {
            if (!signed || (title != null && title.length() > MAX_TITLE)) {
                return false;
            }
            Map<String, Object> c = content();
            c.put("title", filterable(title == null ? "" : title));
            c.putIfAbsent("author", "");
            setContent(c);
            return true;
        }

        @Override
        public boolean hasAuthor() {
            return signed && content().get("author") instanceof String s && !s.isEmpty();
        }

        @Override
        public String getAuthor() {
            return hasAuthor() ? (String) content().get("author") : null;
        }

        @Override
        public void setAuthor(String author) {
            if (!signed) {
                return;
            }
            Map<String, Object> c = content();
            c.put("author", author == null ? "" : author);
            c.putIfAbsent("title", filterable(""));
            setContent(c);
        }

        @Override
        public boolean hasGeneration() {
            return signed && content().containsKey("generation");
        }

        @Override
        public Generation getGeneration() {
            int g = Nbt.intOr(content().get("generation"), 0);
            return Generation.values()[Math.max(0, Math.min(g, Generation.values().length - 1))];
        }

        @Override
        public void setGeneration(Generation generation) {
            if (!signed) {
                return;
            }
            Map<String, Object> c = content();
            if (generation == null) {
                c.remove("generation");
            } else {
                c.put("generation", generation.ordinal());
            }
            setContent(c);
        }

        @Override
        public BookMeta.Spigot spigot() {
            return spigot;
        }

        @Override
        public Book clone() {
            return (Book) copy();
        }
    }
}
