package dev.ferraloom.bukkit;

import java.io.File;
import java.util.List;
import java.util.Map;
import java.util.logging.Logger;
import org.bukkit.Bukkit;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.ItemFlag;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.inventory.meta.Damageable;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.SkullMeta;
import org.bukkit.persistence.PersistentDataType;

/**
 * The item conversion's own checks, run by build.py against the API jar: the bytes src/inventory.rs
 * gives become Bukkit items and come back with what the bridge does not model untouched.
 */
public final class ItemsTest {
    public static void main(String[] args) {
        Bukkit.setServer(new FerraloomServer(Logger.getLogger("ItemsTest"), new File("."), "test", new String[0],
                new int[0], 20, 25565, "", true));
        unmodelledComponentsSurviveAnEdit();
        anUntouchedItemComesBackTheSame();
        coloredNamesAreCraftBukkits();
        pluginDataLivesInCustomData();
        flagsHideComponents();
        typedMetasFollowTheMaterial();
        System.out.println("ItemsTest: all passed");
    }

    static void check(boolean ok, String what) {
        if (!ok) {
            throw new AssertionError(what);
        }
    }

    /** A sword from the server: food (unmodelled), a removed default and plugin data. */
    static byte[] serverSword() {
        Map<String, Object> food = Nbt.map();
        food.put("nutrition", 4);
        food.put("saturation", 1.5f);
        Map<String, Object> custom = Nbt.map();
        custom.put("other_mod", (byte) 7);
        Map<String, Object> components = Nbt.map();
        components.put("minecraft:custom_name", "Blade");
        components.put("minecraft:food", food);
        components.put("!minecraft:tool", Nbt.map());
        components.put("minecraft:custom_data", custom);
        components.put("minecraft:damage", 12);
        Map<String, Object> tag = Nbt.map();
        tag.put("id", "minecraft:diamond_sword");
        tag.put("count", 1);
        tag.put("components", components);
        return Nbt.write(tag);
    }

    static Map<String, Object> components(ItemStack stack) {
        return Nbt.compound(Nbt.compound(Nbt.read(Items.write(stack))).get("components"));
    }

    static void unmodelledComponentsSurviveAnEdit() {
        ItemStack sword = Items.read(serverSword());
        check(sword.getType() == Material.DIAMOND_SWORD && sword.getAmount() == 1, "type and amount");
        check(sword.getDurability() == 12, "damage reads as durability");
        ItemMeta meta = sword.getItemMeta();
        meta.setLore(List.of("Sharp"));
        meta.addEnchant(Enchantment.SHARPNESS, 5, true);
        meta.getPersistentDataContainer().set(new NamespacedKey("shop", "price"), PersistentDataType.LONG, 250L);
        sword.setItemMeta(meta);
        Map<String, Object> c = components(sword);
        check(Nbt.same(c.get("minecraft:food"), Nbt.compound(Nbt.compound(Nbt.read(serverSword())).get("components")).get("minecraft:food")),
                "food unchanged");
        check(c.containsKey("!minecraft:tool"), "the removed default stays removed");
        Map<String, Object> custom = Nbt.compound(c.get("minecraft:custom_data"));
        check(Byte.valueOf((byte) 7).equals(custom.get("other_mod")), "another mod's custom data stays");
        check(Long.valueOf(250).equals(Nbt.compound(custom.get("PublicBukkitValues")).get("shop:price")), "plugin data");
        check(Nbt.compound(c.get("minecraft:enchantments")).get("minecraft:sharpness").equals(5), "enchantment");
        check(Nbt.items(c.get("minecraft:lore")).size() == 1, "lore");
    }

    static void anUntouchedItemComesBackTheSame() {
        ItemStack sword = Items.read(serverSword());
        check(Nbt.same(Nbt.read(Items.write(sword)), Nbt.read(serverSword())), "read then written, the same item");
        check(sword.isSimilar(Items.read(serverSword())) && sword.equals(sword.clone()), "equal copies");
        check(Items.write(null) == null && Items.write(new ItemStack(Material.AIR)) == null, "empty is null");
        check(!new ItemStack(Material.STONE).hasItemMeta(), "a fresh item has no meta");
    }

    static void coloredNamesAreCraftBukkits() {
        ItemStack chest = new ItemStack(Material.CHEST);
        ItemMeta meta = chest.getItemMeta();
        meta.setDisplayName("§6Shop §lNow");
        chest.setItemMeta(meta);
        Map<String, Object> name = Nbt.compound(components(chest).get("minecraft:custom_name"));
        Map<String, Object> first = Nbt.compound(Nbt.items(name.get("extra")).get(0));
        check("Shop ".equals(first.get("text")) && "gold".equals(first.get("color")), "a colour code colours");
        check(Byte.valueOf((byte) 0).equals(first.get("italic")), "and resets the formatting");
        check(Nbt.compound(Nbt.items(name.get("extra")).get(1)).get("bold").equals((byte) 1), "bold carries on");
        check("§6Shop §6§lNow".equals(chest.getItemMeta().getDisplayName()), "the name reads back as legacy text");
    }

    static void pluginDataLivesInCustomData() {
        ItemStack gem = new ItemStack(Material.EMERALD, 3);
        ItemMeta meta = gem.getItemMeta();
        NamespacedKey id = new NamespacedKey("gems", "id");
        meta.getPersistentDataContainer().set(id, PersistentDataType.STRING, "ruby");
        meta.getPersistentDataContainer().set(new NamespacedKey("gems", "level"), PersistentDataType.INTEGER, 2);
        gem.setItemMeta(meta);
        ItemStack back = Items.read(Items.write(gem));
        check("ruby".equals(back.getItemMeta().getPersistentDataContainer().get(id, PersistentDataType.STRING)), "string");
        check(back.getItemMeta().getPersistentDataContainer().has(id, PersistentDataType.STRING), "has");
        check(!back.getItemMeta().getPersistentDataContainer().has(id, PersistentDataType.INTEGER), "not as another type");
        check(back.getAmount() == 3 && back.isSimilar(gem), "the same gem");
    }

    static void flagsHideComponents() {
        ItemStack sword = new ItemStack(Material.IRON_SWORD);
        ItemMeta meta = sword.getItemMeta();
        meta.addItemFlags(ItemFlag.HIDE_ENCHANTS, ItemFlag.HIDE_ATTRIBUTES);
        meta.setUnbreakable(true);
        sword.setItemMeta(meta);
        Map<String, Object> display = Nbt.compound(components(sword).get("minecraft:tooltip_display"));
        check(Nbt.items(display.get("hidden_components")).equals(List.of("minecraft:enchantments", "minecraft:attribute_modifiers")),
                "hidden components");
        ItemMeta back = sword.getItemMeta();
        check(back.hasItemFlag(ItemFlag.HIDE_ENCHANTMENTS) && back.getItemFlags().contains(ItemFlag.HIDE_ATTRIBUTES), "flags read back");
        check(back.isUnbreakable(), "unbreakable");
        Damageable damage = (Damageable) back;
        damage.setDamage(40);
        sword.setItemMeta(damage);
        check(sword.getDurability() == 40, "damage");
    }

    static void typedMetasFollowTheMaterial() {
        ItemStack book = new ItemStack(Material.WRITTEN_BOOK);
        BookMeta meta = (BookMeta) book.getItemMeta();
        check(meta.setTitle("Rules"), "title");
        meta.setAuthor("Server");
        meta.addPage("§cNo griefing\nBe kind");
        book.setItemMeta(meta);
        BookMeta back = (BookMeta) Items.read(Items.write(book)).getItemMeta();
        check("Rules".equals(back.getTitle()) && "Server".equals(back.getAuthor()), "title and author");
        check(back.getPageCount() == 1 && back.getPage(1).equals("§cNo griefing\nBe kind"), "page");
        check(back.spigot().getPages().size() == 1, "pages as components");
        check(new ItemStack(Material.PLAYER_HEAD).getItemMeta() instanceof SkullMeta, "heads have skull meta");
        check(!(new ItemStack(Material.STONE).getItemMeta() instanceof SkullMeta), "stone does not");
        SkullMeta head = (SkullMeta) new ItemStack(Material.PLAYER_HEAD).getItemMeta();
        head.setOwner("Notch");
        check("Notch".equals(head.getOwner()) && head.hasOwner(), "owner");
    }
}
