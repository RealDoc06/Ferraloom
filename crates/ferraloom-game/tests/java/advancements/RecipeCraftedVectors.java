import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.advancements.triggers.Criterion;
import net.minecraft.advancements.triggers.RecipeCraftedTrigger;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.*;

/** Actual RecipeCraftedTrigger matcher observes list order and receiver multiplicity. */
public final class RecipeCraftedVectors {
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        var rows = new JsonArray();
        String[][] cases = {
            {"[{}, {\"items\":\"stone\"}]", "[\"stone\",\"apple\"]", "[0,1]", "crafting_table"},
            {"[{}, {\"items\":\"stone\"}]", "[\"stone\",\"apple\"]", "[1,0]", "crafting_table"},
            {"[{\"items\":\"stone\"},{\"items\":\"stone\"}]", "[\"stone\"]", "[0]", "crafting_table"},
            {"[{\"items\":\"stone\"},{\"items\":\"stone\"}]", "[\"stone\"]", "[0,0]", "crafting_table"},
            {"[{\"items\":\"stone\"}]", "[\"stone\"]", "[0]", "furnace"},
            {"[]", "[]", "[]", "crafting_table"}
        };
        var method = RecipeCraftedTrigger.TriggerInstance.class.getDeclaredMethod("matches", ResourceKey.class, List.class);
        method.setAccessible(true);
        for (String[] sample : cases) {
            var input = new JsonObject(); input.addProperty("trigger", "recipe_crafted");
            var conditions = new JsonObject(); conditions.addProperty("recipes", "crafting_table");
            conditions.add("ingredients", AdvancementVectors.json(sample[0])); input.add("conditions", conditions);
            var criterion = Criterion.CODEC.parse(AdvancementVectors.ops, input).getOrThrow();
            var objects = new ArrayList<ItemStack>();
            var serialized = new JsonArray();
            for (var item : AdvancementVectors.json(sample[1]).getAsJsonArray()) {
                var stack = new ItemStack(item.getAsString().equals("stone") ? Items.STONE : Items.APPLE, 64);
                objects.add(stack); serialized.add(ItemStack.CODEC.encodeStart(AdvancementVectors.ops, stack).getOrThrow());
            }
            var indices = AdvancementVectors.json(sample[2]).getAsJsonArray();
            var used = new ArrayList<ItemStack>(); for (var index : indices) used.add(objects.get(index.getAsInt()));
            var key = ResourceKey.create(Registries.RECIPE, AdvancementVectors.id(sample[3]));
            boolean matches = (boolean)method.invoke(criterion.triggerInstance(), key, used);
            var row = new JsonObject(); row.add("input", input); row.add("objects", serialized); row.add("used", indices);
            row.addProperty("recipe", key.identifier().toString()); row.addProperty("matches", matches); rows.add(row);
        }
        try (var output = new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            output.write(AdvancementVectors.JSON.toJson(rows).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        System.out.println("recipe_crafted_receiver_lists: " + rows.size());
    }
}
