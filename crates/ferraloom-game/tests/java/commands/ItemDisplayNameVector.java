import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.component.DataComponents;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ItemStack;

/** Calls the real ItemStack name methods and encodes their exact components. */
public final class ItemDisplayNameVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var registries = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p -> p.apply());
        var ops = registries.createSerializationContext(JsonOps.INSTANCE);
        var rows = new JsonArray();
        var inputs = new ArrayList<String>(List.of(
            "{\"id\":\"minecraft:stone\"}",
            "{\"id\":\"minecraft:stone\",\"count\":64}",
            "{\"id\":\"minecraft:stone\",\"count\":99}",
            "{\"id\":\"minecraft:enchanted_golden_apple\"}",
            "{\"id\":\"minecraft:dragon_egg\"}",
            "{\"id\":\"minecraft:stone\",\"components\":{\"minecraft:custom_name\":\"Named stone\"}}",
            "{\"id\":\"minecraft:stone\",\"components\":{\"minecraft:custom_name\":{\"text\":\"Styled\",\"italic\":false,\"color\":\"red\"}}}",
            "{\"id\":\"minecraft:stone\",\"components\":{\"minecraft:item_name\":\"Item override\"}}",
            "{\"id\":\"minecraft:stone\",\"components\":{\"!minecraft:item_name\":{}}}",
            "{\"id\":\"minecraft:written_book\",\"components\":{\"minecraft:written_book_content\":{\"title\":{\"raw\":\"Raw title\",\"filtered\":\"Filtered title\"},\"author\":\"Author\"}}}",
            "{\"id\":\"minecraft:written_book\",\"components\":{\"minecraft:written_book_content\":{\"title\":\"Title\",\"author\":\"Author\"},\"minecraft:custom_name\":\"Custom book\"}}",
            "{\"id\":\"minecraft:compass\",\"components\":{\"minecraft:lodestone_tracker\":{\"tracked\":false}}}",
            "{\"id\":\"minecraft:enchanted_book\",\"components\":{\"minecraft:stored_enchantments\":{\"minecraft:sharpness\":1}}}"
        ));
        for (String rarity : List.of("common", "uncommon", "rare", "epic")) {
            for (boolean enchanted : List.of(false, true)) {
                inputs.add("{\"id\":\"minecraft:stone\",\"components\":{\"minecraft:rarity\":\"" + rarity + "\""
                    + (enchanted ? ",\"minecraft:enchantments\":{\"minecraft:sharpness\":1}" : "") + "}}");
            }
        }
        for (String blank : List.of("", " ", "\t\n", "\u00a0", "\u001c", "\u0085", "\u2007", "\u202f", "\u3000")) {
            var input = new JsonObject(); input.addProperty("id", "minecraft:written_book");
            var components = new JsonObject(); var book = new JsonObject();
            book.addProperty("title", blank); book.addProperty("author", "Author");
            components.add("minecraft:written_book_content", book); input.add("components", components);
            inputs.add(input.toString());
        }
        for (int index = -2; index < inputs.size(); index++) {
            var input = JsonParser.parseString(inputs.get(Math.max(0, index)));
            var stack = index == -2 ? ItemStack.EMPTY : ItemStack.CODEC.parse(ops, input).getOrThrow();
            if (index == -1) stack.setCount(0);
            var row = new JsonObject(); row.addProperty("case", index);
            row.add("input", input); row.addProperty("empty", index == -2); row.addProperty("depleted", index == -1);
            row.add("hover", ComponentSerialization.CODEC.encodeStart(ops, stack.getHoverName()).getOrThrow());
            row.add("display", ComponentSerialization.CODEC.encodeStart(ops, stack.getDisplayName()).getOrThrow());
            rows.add(row);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows) + "\n");
    }
}
