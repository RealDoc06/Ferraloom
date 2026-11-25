package dev.ferraloom.bukkit;

import com.google.common.collect.ImmutableList;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableMultimap;
import com.google.common.collect.LinkedHashMultimap;
import com.google.common.collect.Multimap;
import dev.ferraloom.bukkit.stub.StubItemMeta;
import java.util.ArrayList;
import java.util.Base64;
import java.util.Collection;
import java.util.EnumSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import org.bukkit.Color;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeModifier;
import org.bukkit.configuration.serialization.ConfigurationSerializable;
import org.bukkit.configuration.serialization.ConfigurationSerialization;
import org.bukkit.configuration.serialization.SerializableAs;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.EquipmentSlotGroup;
import org.bukkit.inventory.ItemFlag;
import org.bukkit.inventory.ItemRarity;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.Damageable;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.Repairable;
import org.bukkit.inventory.meta.components.CustomModelDataComponent;
import org.bukkit.persistence.PersistentDataContainer;

/**
 * CraftMetaItem for Ferraloom: an item's data component patch, as the server holds it
 * ({@code components} of ItemStack.CODEC, ids with their namespace, a removed default as
 * {@code !id}). Every getter reads the patch and every setter writes it, so what the bridge does
 * not model (food, tools, entity data, mods' components...) stays as it came, and an item read
 * from the server and given back unchanged is the same item.
 *
 * <p>The typed metas a material asks for ({@link FerraloomItemFactory#getItemMeta}) extend this one.
 */
class FerraloomItemMeta extends StubItemMeta implements Damageable, Repairable {
    /** The component patch, by id ({@code minecraft:custom_name}). */
    final Map<String, Object> components;
    /** The {@code PublicBukkitValues} of {@code minecraft:custom_data}, made on first ask. */
    private FerraloomPersistentData persistentData;

    FerraloomItemMeta(Map<String, Object> components) {
        this.components = components;
    }

    // The patch.

    Object get(String id) {
        return components.get("minecraft:" + id);
    }

    boolean has(String id) {
        return components.containsKey("minecraft:" + id);
    }

    /** Sets a component (none: back to the item's default). */
    void put(String id, Object value) {
        components.remove("!minecraft:" + id);
        if (value == null) {
            components.remove("minecraft:" + id);
        } else {
            components.put("minecraft:" + id, value);
        }
    }

    Map<String, Object> compound(String id) {
        return Nbt.compound(get(id));
    }

    /** The patch as it goes to the server, with the plugin data container written in. */
    Map<String, Object> patch() {
        if (persistentData != null) {
            Map<String, Object> custom = compound("custom_data");
            custom = custom == null ? Nbt.map() : Nbt.copy(custom);
            if (persistentData.isEmpty()) {
                custom.remove(FerraloomPersistentData.BUKKIT);
            } else {
                custom.put(FerraloomPersistentData.BUKKIT, Nbt.copy(persistentData.tags));
            }
            put("custom_data", custom.isEmpty() ? null : custom);
        }
        return components;
    }

    boolean isEmpty() {
        return patch().isEmpty();
    }

    // Names and lore.

    @Override
    public boolean hasDisplayName() {
        return has("custom_name");
    }

    @Override
    public String getDisplayName() {
        return Texts.toLegacy(get("custom_name"));
    }

    @Override
    public void setDisplayName(String name) {
        put("custom_name", Texts.fromLegacyOrNull(name));
    }

    @Override
    public boolean hasItemName() {
        return has("item_name");
    }

    @Override
    public String getItemName() {
        return Texts.toLegacy(get("item_name"));
    }

    @Override
    public void setItemName(String name) {
        put("item_name", Texts.fromLegacyOrNull(name));
    }

    /** Gone from items since 1.20.5; CraftMetaItem answers the same. */
    @Override
    public boolean hasLocalizedName() {
        return false;
    }

    @Override
    public String getLocalizedName() {
        return null;
    }

    @Override
    public void setLocalizedName(String name) {
    }

    @Override
    public boolean hasLore() {
        return !Nbt.items(get("lore")).isEmpty();
    }

    @Override
    public List<String> getLore() {
        if (!hasLore()) {
            return null;
        }
        List<String> lore = new ArrayList<>();
        for (Object line : Nbt.items(get("lore"))) {
            lore.add(Texts.toLegacy(line));
        }
        return lore;
    }

    @Override
    public void setLore(List<String> lore) {
        if (lore == null || lore.isEmpty()) {
            put("lore", null);
            return;
        }
        List<Object> lines = new ArrayList<>();
        for (String line : lore) {
            lines.add(Texts.fromLegacy(line == null ? "" : line));
        }
        put("lore", new Nbt.ListTag(10, lines));
    }

    // Custom model data.

    @Override
    public boolean hasCustomModelData() {
        Map<String, Object> c = compound("custom_model_data");
        return c != null && !Nbt.items(c.get("floats")).isEmpty();
    }

    @Override
    public int getCustomModelData() {
        if (!hasCustomModelData()) {
            throw new IllegalStateException("We don't have CustomModelData! Check hasCustomModelData first!");
        }
        return ((Number) Nbt.items(compound("custom_model_data").get("floats")).get(0)).intValue();
    }

    @Override
    public void setCustomModelData(Integer data) {
        if (data == null) {
            put("custom_model_data", null);
            return;
        }
        Map<String, Object> c = Nbt.map();
        c.put("floats", new Nbt.ListTag(5, List.of(data.floatValue())));
        put("custom_model_data", c);
    }

    @Override
    public boolean hasCustomModelDataComponent() {
        return has("custom_model_data");
    }

    @Override
    public CustomModelDataComponent getCustomModelDataComponent() {
        Map<String, Object> c = compound("custom_model_data");
        return new ModelData(c == null ? Nbt.map() : Nbt.copy(c));
    }

    @Override
    public void setCustomModelDataComponent(CustomModelDataComponent data) {
        if (data == null) {
            put("custom_model_data", null);
            return;
        }
        Map<String, Object> c = Nbt.map();
        List<Object> colors = new ArrayList<>();
        data.getColors().forEach(color -> colors.add(color.asRGB()));
        putList(c, "floats", 5, new ArrayList<>(data.getFloats()));
        putList(c, "flags", 1, data.getFlags().stream().map(f -> (Object) Nbt.bool(f)).toList());
        putList(c, "strings", 8, new ArrayList<>(data.getStrings()));
        putList(c, "colors", 3, colors);
        put("custom_model_data", c);
    }

    private static void putList(Map<String, Object> c, String key, int type, List<?> items) {
        if (!items.isEmpty()) {
            c.put(key, new Nbt.ListTag(type, new ArrayList<>(items)));
        }
    }

    /** CraftCustomModelDataComponent: a copy; set it back with {@link #setCustomModelDataComponent}. */
    @SerializableAs("CustomModelData")
    static final class ModelData implements CustomModelDataComponent {
        private final Map<String, Object> tag;

        ModelData(Map<String, Object> tag) {
            this.tag = tag;
        }

        @SuppressWarnings("unchecked")
        private <T> List<T> list(String key) {
            return (List<T>) List.copyOf(Nbt.items(tag.get(key)));
        }

        @Override
        public List<Float> getFloats() {
            return list("floats");
        }

        @Override
        public void setFloats(List<Float> floats) {
            putList(tag, "floats", 5, floats);
        }

        @Override
        public List<Boolean> getFlags() {
            return Nbt.items(tag.get("flags")).stream().map(b -> Nbt.intOr(b, 0) != 0).toList();
        }

        @Override
        public void setFlags(List<Boolean> flags) {
            putList(tag, "flags", 1, flags.stream().map(f -> (Object) Nbt.bool(f)).toList());
        }

        @Override
        public List<String> getStrings() {
            return list("strings");
        }

        @Override
        public void setStrings(List<String> strings) {
            putList(tag, "strings", 8, strings);
        }

        @Override
        public List<Color> getColors() {
            return Nbt.items(tag.get("colors")).stream().map(c -> Color.fromRGB(Nbt.intOr(c, 0) & 0xFFFFFF)).toList();
        }

        @Override
        public void setColors(List<Color> colors) {
            putList(tag, "colors", 3, colors.stream().map(c -> (Object) c.asRGB()).toList());
        }

        @Override
        public Map<String, Object> serialize() {
            return Map.of("floats", getFloats(), "flags", getFlags(), "strings", getStrings(), "colors", getColors());
        }
    }

    // Enchantments: `minecraft:enchantments`, `{id: level}`.

    /** The enchantments of a compound of levels, those the registry knows, in order. */
    static Map<Enchantment, Integer> enchants(Map<String, Object> levels) {
        Map<Enchantment, Integer> out = new LinkedHashMap<>();
        if (levels != null) {
            levels.forEach((id, level) -> {
                NamespacedKey key = NamespacedKey.fromString(id);
                Enchantment e = key == null ? null : Registry.ENCHANTMENT.get(key);
                if (e != null) {
                    out.put(e, Nbt.intOr(level, 1));
                }
            });
        }
        return out;
    }

    /** CraftMetaItem.addEnchant over the compound {@code id}: true when it changed. */
    boolean addEnchant(String id, Enchantment e, int level, boolean ignoreLevelRestriction) {
        if (!ignoreLevelRestriction && (level < e.getStartLevel() || level > e.getMaxLevel())) {
            return false;
        }
        Map<String, Object> levels = compound(id);
        levels = levels == null ? Nbt.map() : Nbt.copy(levels);
        Object old = levels.put(e.getKey().toString(), level);
        put(id, levels);
        return old == null || Nbt.intOr(old, 0) != level;
    }

    boolean removeEnchant(String id, Enchantment e) {
        Map<String, Object> levels = compound(id);
        if (levels == null || !levels.containsKey(e.getKey().toString())) {
            return false;
        }
        levels = Nbt.copy(levels);
        levels.remove(e.getKey().toString());
        put(id, levels.isEmpty() ? null : levels);
        return true;
    }

    boolean conflicts(String id, Enchantment e) {
        for (Enchantment other : enchants(compound(id)).keySet()) {
            if (!other.equals(e) && other.conflictsWith(e)) {
                return true;
            }
        }
        return false;
    }

    @Override
    public boolean hasEnchants() {
        return !enchants(compound("enchantments")).isEmpty();
    }

    @Override
    public boolean hasEnchant(Enchantment e) {
        Map<String, Object> levels = compound("enchantments");
        return levels != null && levels.containsKey(e.getKey().toString());
    }

    @Override
    public int getEnchantLevel(Enchantment e) {
        Map<String, Object> levels = compound("enchantments");
        return levels == null ? 0 : Nbt.intOr(levels.get(e.getKey().toString()), 0);
    }

    @Override
    public Map<Enchantment, Integer> getEnchants() {
        return ImmutableMap.copyOf(enchants(compound("enchantments")));
    }

    @Override
    public boolean addEnchant(Enchantment e, int level, boolean ignoreLevelRestriction) {
        return addEnchant("enchantments", e, level, ignoreLevelRestriction);
    }

    @Override
    public boolean removeEnchant(Enchantment e) {
        return removeEnchant("enchantments", e);
    }

    @Override
    public void removeEnchantments() {
        put("enchantments", null);
    }

    @Override
    public boolean hasConflictingEnchant(Enchantment e) {
        return conflicts("enchantments", e);
    }

    @Override
    public boolean hasEnchantable() {
        return has("enchantable");
    }

    @Override
    public int getEnchantable() {
        Map<String, Object> c = compound("enchantable");
        return c == null ? 0 : Nbt.intOr(c.get("value"), 0);
    }

    @Override
    public void setEnchantable(Integer value) {
        if (value == null) {
            put("enchantable", null);
            return;
        }
        Map<String, Object> c = Nbt.map();
        c.put("value", value);
        put("enchantable", c);
    }

    // Item flags: `minecraft:tooltip_display`'s hidden components.

    /** The components an item flag hides: one each, but for the pre-1.21.5 flags. */
    static List<String> hides(ItemFlag flag) {
        return switch (flag.name()) {
            case "HIDE_ENCHANTS" -> List.of("minecraft:enchantments");
            case "HIDE_ATTRIBUTES" -> List.of("minecraft:attribute_modifiers");
            case "HIDE_UNBREAKABLE" -> List.of("minecraft:unbreakable");
            case "HIDE_DESTROYS" -> List.of("minecraft:can_break");
            case "HIDE_PLACED_ON" -> List.of("minecraft:can_place_on");
            case "HIDE_DYE" -> List.of("minecraft:dyed_color");
            case "HIDE_ARMOR_TRIM" -> List.of("minecraft:trim");
            case "HIDE_ADDITIONAL_TOOLTIP" -> ADDITIONAL;
            default -> {
                String id = Components.byFlag(flag.name().substring("HIDE_".length()));
                yield id == null ? List.of() : List.of(id);
            }
        };
    }

    /** What 1.21.4's {@code hide_additional_tooltip} hid. */
    private static final List<String> ADDITIONAL = List.of("minecraft:banner_patterns", "minecraft:bees",
            "minecraft:block_entity_data", "minecraft:block_state", "minecraft:bundle_contents",
            "minecraft:charged_projectiles", "minecraft:container", "minecraft:container_loot",
            "minecraft:firework_explosion", "minecraft:fireworks", "minecraft:instrument",
            "minecraft:jukebox_playable", "minecraft:map_id", "minecraft:ominous_bottle_amplifier",
            "minecraft:pot_decorations", "minecraft:potion_contents", "minecraft:stored_enchantments",
            "minecraft:suspicious_stew_effects", "minecraft:written_book_content");

    private List<Object> hidden() {
        Map<String, Object> display = compound("tooltip_display");
        return display == null ? new ArrayList<>() : new ArrayList<>(Nbt.items(display.get("hidden_components")));
    }

    private void setTooltip(boolean hide, List<Object> hidden) {
        Map<String, Object> display = Nbt.map();
        if (hide) {
            display.put("hide_tooltip", Nbt.bool(true));
        }
        if (!hidden.isEmpty()) {
            display.put("hidden_components", new Nbt.ListTag(8, hidden));
        }
        put("tooltip_display", display.isEmpty() ? null : display);
    }

    @Override
    public void addItemFlags(ItemFlag... flags) {
        List<Object> hidden = hidden();
        for (ItemFlag flag : flags) {
            for (String id : hides(flag)) {
                if (!hidden.contains(id)) {
                    hidden.add(id);
                }
            }
        }
        setTooltip(isHideTooltip(), hidden);
    }

    @Override
    public void removeItemFlags(ItemFlag... flags) {
        List<Object> hidden = hidden();
        for (ItemFlag flag : flags) {
            hidden.removeAll(hides(flag));
        }
        setTooltip(isHideTooltip(), hidden);
    }

    @Override
    public Set<ItemFlag> getItemFlags() {
        List<Object> hidden = hidden();
        Set<ItemFlag> flags = EnumSet.noneOf(ItemFlag.class);
        for (ItemFlag flag : ItemFlag.values()) {
            List<String> ids = hides(flag);
            if (!ids.isEmpty() && hidden.containsAll(ids)) {
                flags.add(flag);
            }
        }
        return flags;
    }

    @Override
    public boolean hasItemFlag(ItemFlag flag) {
        List<String> ids = hides(flag);
        return !ids.isEmpty() && hidden().containsAll(ids);
    }

    @Override
    public boolean isHideTooltip() {
        Map<String, Object> display = compound("tooltip_display");
        return display != null && Nbt.intOr(display.get("hide_tooltip"), 0) != 0;
    }

    @Override
    public void setHideTooltip(boolean hide) {
        setTooltip(hide, hidden());
    }

    // Simple components.

    private NamespacedKey key(String id) {
        return get(id) instanceof String s ? NamespacedKey.fromString(s) : null;
    }

    private void putKey(String id, NamespacedKey key) {
        put(id, key == null ? null : key.toString());
    }

    @Override
    public boolean hasTooltipStyle() {
        return has("tooltip_style");
    }

    @Override
    public NamespacedKey getTooltipStyle() {
        return key("tooltip_style");
    }

    @Override
    public void setTooltipStyle(NamespacedKey style) {
        putKey("tooltip_style", style);
    }

    @Override
    public boolean hasItemModel() {
        return has("item_model");
    }

    @Override
    public NamespacedKey getItemModel() {
        return key("item_model");
    }

    @Override
    public void setItemModel(NamespacedKey model) {
        putKey("item_model", model);
    }

    @Override
    public boolean isUnbreakable() {
        return has("unbreakable");
    }

    @Override
    public void setUnbreakable(boolean unbreakable) {
        put("unbreakable", unbreakable ? Nbt.map() : null);
    }

    @Override
    public boolean hasEnchantmentGlintOverride() {
        return has("enchantment_glint_override");
    }

    @Override
    public Boolean getEnchantmentGlintOverride() {
        if (!hasEnchantmentGlintOverride()) {
            throw new IllegalStateException("We don't have enchantment_glint_override! Check hasEnchantmentGlintOverride first!");
        }
        return Nbt.intOr(get("enchantment_glint_override"), 0) != 0;
    }

    @Override
    public void setEnchantmentGlintOverride(Boolean glint) {
        put("enchantment_glint_override", glint == null ? null : Nbt.bool(glint));
    }

    @Override
    public boolean isGlider() {
        return has("glider");
    }

    @Override
    public void setGlider(boolean glider) {
        put("glider", glider ? Nbt.map() : null);
    }

    /** 1.21.2's {@code fire_resistant}: damage resistance to the {@code #minecraft:is_fire} tag. */
    @Override
    public boolean isFireResistant() {
        Map<String, Object> c = compound("damage_resistant");
        return c != null && "#minecraft:is_fire".equals(c.get("types"));
    }

    @Override
    public void setFireResistant(boolean resistant) {
        if (!resistant) {
            if (isFireResistant()) {
                put("damage_resistant", null);
            }
            return;
        }
        Map<String, Object> c = Nbt.map();
        c.put("types", "#minecraft:is_fire");
        put("damage_resistant", c);
    }

    @Override
    public boolean hasMaxStackSize() {
        return has("max_stack_size");
    }

    @Override
    public int getMaxStackSize() {
        if (!hasMaxStackSize()) {
            throw new IllegalStateException("We don't have max_stack_size! Check hasMaxStackSize first!");
        }
        return Nbt.intOr(get("max_stack_size"), 64);
    }

    @Override
    public void setMaxStackSize(Integer size) {
        if (size != null && (size < 1 || size > 99)) {
            throw new IllegalArgumentException("Max stack size must be between 1 and 99");
        }
        put("max_stack_size", size);
    }

    @Override
    public boolean hasAdditionalTradeCost() {
        return has("additional_trade_cost");
    }

    @Override
    public int getAdditionalTradeCost() {
        return Nbt.intOr(get("additional_trade_cost"), 0);
    }

    @Override
    public void setAdditionalTradeCost(Integer cost) {
        put("additional_trade_cost", cost);
    }

    @Override
    public boolean hasRarity() {
        return has("rarity");
    }

    @Override
    public ItemRarity getRarity() {
        if (!(get("rarity") instanceof String s)) {
            throw new IllegalStateException("We don't have rarity! Check hasRarity first!");
        }
        return ItemRarity.valueOf(s.toUpperCase(Locale.ROOT));
    }

    @Override
    public void setRarity(ItemRarity rarity) {
        put("rarity", rarity == null ? null : rarity.name().toLowerCase(Locale.ROOT));
    }

    @Override
    public boolean hasMinimumAttackCharge() {
        return has("minimum_attack_charge");
    }

    @Override
    public float getMinimumAttackCharge() {
        return get("minimum_attack_charge") instanceof Number n ? n.floatValue() : 0;
    }

    @Override
    public void setMinimumAttackCharge(Float charge) {
        put("minimum_attack_charge", charge);
    }

    @Override
    public boolean hasUseRemainder() {
        return has("use_remainder");
    }

    @Override
    public ItemStack getUseRemainder() {
        return Items.fromTag(compound("use_remainder"));
    }

    @Override
    public void setUseRemainder(ItemStack remainder) {
        put("use_remainder", Items.toTag(remainder));
    }

    // Attribute modifiers: `minecraft:attribute_modifiers`, a list of {type, id, amount, operation, slot}.

    private static final String[] OPERATIONS = {"add_value", "add_multiplied_base", "add_multiplied_total"};

    private Multimap<Attribute, AttributeModifier> modifiers() {
        Multimap<Attribute, AttributeModifier> out = LinkedHashMultimap.create();
        for (Object entry : Nbt.items(get("attribute_modifiers"))) {
            Map<String, Object> m = Nbt.compound(entry);
            if (m == null || !(m.get("type") instanceof String type) || !(m.get("id") instanceof String id)) {
                continue;
            }
            NamespacedKey typeKey = NamespacedKey.fromString(type);
            Attribute attribute = typeKey == null ? null : Registry.ATTRIBUTE.get(typeKey);
            NamespacedKey key = NamespacedKey.fromString(id);
            int op = List.of(OPERATIONS).indexOf(m.get("operation"));
            if (attribute == null || key == null || op < 0) {
                continue;
            }
            EquipmentSlotGroup slot = m.get("slot") instanceof String s ? EquipmentSlotGroup.getByName(s) : null;
            double amount = m.get("amount") instanceof Number n ? n.doubleValue() : 0;
            out.put(attribute, new AttributeModifier(key, amount, AttributeModifier.Operation.values()[op],
                    slot == null ? EquipmentSlotGroup.ANY : slot));
        }
        return out;
    }

    private void setModifiers(Multimap<Attribute, AttributeModifier> modifiers) {
        if (modifiers == null || modifiers.isEmpty()) {
            put("attribute_modifiers", null);
            return;
        }
        List<Object> list = new ArrayList<>();
        modifiers.forEach((attribute, modifier) -> {
            Map<String, Object> m = Nbt.map();
            m.put("type", attribute.getKey().toString());
            m.put("id", modifier.getKey().toString());
            m.put("amount", modifier.getAmount());
            m.put("operation", OPERATIONS[modifier.getOperation().ordinal()]);
            m.put("slot", modifier.getSlotGroup().toString());
            list.add(m);
        });
        put("attribute_modifiers", new Nbt.ListTag(10, list));
    }

    @Override
    public boolean hasAttributeModifiers() {
        return !modifiers().isEmpty();
    }

    @Override
    public Multimap<Attribute, AttributeModifier> getAttributeModifiers() {
        Multimap<Attribute, AttributeModifier> m = modifiers();
        return m.isEmpty() ? null : ImmutableMultimap.copyOf(m);
    }

    @Override
    public Multimap<Attribute, AttributeModifier> getAttributeModifiers(EquipmentSlot slot) {
        Multimap<Attribute, AttributeModifier> out = LinkedHashMultimap.create();
        modifiers().forEach((a, m) -> {
            if (m.getSlotGroup().test(slot)) {
                out.put(a, m);
            }
        });
        return ImmutableMultimap.copyOf(out);
    }

    @Override
    public Collection<AttributeModifier> getAttributeModifiers(Attribute attribute) {
        Multimap<Attribute, AttributeModifier> m = modifiers();
        return m.isEmpty() ? null : ImmutableList.copyOf(m.get(attribute));
    }

    @Override
    public boolean addAttributeModifier(Attribute attribute, AttributeModifier modifier) {
        Multimap<Attribute, AttributeModifier> m = modifiers();
        for (AttributeModifier existing : m.values()) {
            if (existing.getKey().equals(modifier.getKey())) {
                throw new IllegalArgumentException("Cannot register AttributeModifier. Modifier is already applied! " + modifier);
            }
        }
        m.put(attribute, modifier);
        setModifiers(m);
        return true;
    }

    @Override
    public void setAttributeModifiers(Multimap<Attribute, AttributeModifier> modifiers) {
        setModifiers(modifiers == null ? null : LinkedHashMultimap.create(modifiers));
    }

    @Override
    public boolean removeAttributeModifier(Attribute attribute) {
        Multimap<Attribute, AttributeModifier> m = modifiers();
        boolean removed = !m.removeAll(attribute).isEmpty();
        setModifiers(m);
        return removed;
    }

    @Override
    public boolean removeAttributeModifier(EquipmentSlot slot) {
        Multimap<Attribute, AttributeModifier> m = modifiers();
        boolean removed = m.entries().removeIf(e -> e.getValue().getSlotGroup().test(slot));
        setModifiers(m);
        return removed;
    }

    @Override
    public boolean removeAttributeModifier(Attribute attribute, AttributeModifier modifier) {
        Multimap<Attribute, AttributeModifier> m = modifiers();
        boolean removed = m.entries().removeIf(e -> e.getKey().equals(attribute)
                && e.getValue().getKey().equals(modifier.getKey()));
        setModifiers(m);
        return removed;
    }

    // Damageable and Repairable.

    @Override
    public boolean hasDamage() {
        return getDamage() > 0;
    }

    @Override
    public int getDamage() {
        return Nbt.intOr(get("damage"), 0);
    }

    @Override
    public void setDamage(int damage) {
        put("damage", damage > 0 ? (Object) damage : null);
    }

    @Override
    public boolean hasMaxDamage() {
        return has("max_damage");
    }

    @Override
    public int getMaxDamage() {
        if (!hasMaxDamage()) {
            throw new IllegalStateException("We don't have max_damage! Check hasMaxDamage first!");
        }
        return Nbt.intOr(get("max_damage"), 0);
    }

    @Override
    public void setMaxDamage(Integer max) {
        put("max_damage", max);
    }

    @Override
    public boolean hasRepairCost() {
        return getRepairCost() > 0;
    }

    @Override
    public int getRepairCost() {
        return Nbt.intOr(get("repair_cost"), 0);
    }

    @Override
    public void setRepairCost(int cost) {
        put("repair_cost", cost > 0 ? (Object) cost : null);
    }

    // Plugin data.

    @Override
    public PersistentDataContainer getPersistentDataContainer() {
        if (persistentData == null) {
            Map<String, Object> custom = compound("custom_data");
            Map<String, Object> bukkit = custom == null ? null : Nbt.compound(custom.get(FerraloomPersistentData.BUKKIT));
            persistentData = new FerraloomPersistentData(bukkit == null ? Nbt.map() : Nbt.copy(bukkit));
        }
        return persistentData;
    }

    // Strings, serialization, identity.

    /** The patch as the {@code /give} syntax writes it: {@code [minecraft:custom_name="...",...]}. */
    @Override
    public String getAsString() {
        StringBuilder s = new StringBuilder("[");
        patch().forEach((id, value) -> s.append(s.length() > 1 ? "," : "").append(id).append('=').append(Nbt.snbt(value)));
        return s.append(']').toString();
    }

    @Override
    public String getAsComponentString() {
        return getAsString();
    }

    /** ItemStack.deserialize sets the data version it was saved with; the patch is current already. */
    @Override
    public void setVersion(int version) {
    }

    /** The meta type CraftMetaItem names in {@code meta-type}, for configurations. */
    String metaType() {
        return "UNSPECIFIC";
    }

    /** As CraftMetaItem.serialize: what YAML keeps of an item's meta. The patch goes whole, as {@code internal}. */
    @Override
    public Map<String, Object> serialize() {
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("meta-type", metaType());
        if (hasDisplayName()) {
            out.put("display-name", getDisplayName());
        }
        if (hasLore()) {
            out.put("lore", getLore());
        }
        out.put("internal", Base64.getEncoder().encodeToString(Nbt.write(patch())));
        return out;
    }

    /** CraftMetaItem.SerializableMeta: {@code ==: ItemMeta} in YAML, read back by meta type. */
    @SerializableAs("ItemMeta")
    static final class Serialized implements ConfigurationSerializable {
        static {
            ConfigurationSerialization.registerClass(Serialized.class, "ItemMeta");
        }

        private Serialized() {
        }

        static void register() {
            // The static block registers the alias once.
        }

        public static ItemMeta deserialize(Map<String, Object> map) {
            Map<String, Object> patch = Nbt.map();
            if (map.get("internal") instanceof String internal) {
                Map<String, Object> read = Nbt.compound(Nbt.read(Base64.getDecoder().decode(internal)));
                if (read != null) {
                    patch.putAll(read);
                }
            }
            FerraloomItemMeta meta = FerraloomItemFactory.metaOfType(String.valueOf(map.get("meta-type")), patch);
            if (map.get("internal") == null) {
                if (map.get("display-name") instanceof String name) {
                    meta.setDisplayName(name);
                }
                if (map.get("lore") instanceof List<?> lore) {
                    meta.setLore(lore.stream().map(String::valueOf).toList());
                }
            }
            return meta;
        }

        @Override
        public Map<String, Object> serialize() {
            throw new UnsupportedOperationException("serialized through the meta");
        }
    }

    /** A copy of this meta of {@code type}'s class, its patch copied. */
    FerraloomItemMeta copy() {
        return FerraloomItemFactory.metaOfType(metaType(), Nbt.copy(patch()));
    }

    @Override
    public FerraloomItemMeta clone() {
        return copy();
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FerraloomItemMeta m && Nbt.same(patch(), m.patch());
    }

    @Override
    public int hashCode() {
        return Nbt.hash(patch());
    }

    @Override
    public String toString() {
        return metaType() + "_META:" + getAsString();
    }
}
