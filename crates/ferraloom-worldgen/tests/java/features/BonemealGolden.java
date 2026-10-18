import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.tags.FeatureTags;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;

/** Real generation-settings lists and composite holder traversal, without world IO. */
public final class BonemealGolden {
    static void generate(RegistryAccess.Frozen registries, Path out) throws Exception {
        var result = new JsonObject();
        var features = registries.lookupOrThrow(Registries.FEATURE);
        var biomes = new JsonObject();
        for (var biome : registries.lookupOrThrow(Registries.BIOME).listElements().toList()) {
            var values = new JsonArray();
            for (var feature : biome.value().getGenerationSettings().getBoneMealFeatures()) {
                values.add(features.getKey(feature).toString());
            }
            biomes.add(biome.key().identifier().toString(), values);
        }
        result.add("biomes", biomes);
        String a = "{feature:'minecraft:flower_default',placement:[]}";
        String b = "{feature:'minecraft:flower_swamp',placement:[]}";
        var configs = List.of(
            "{type:'minecraft:random_boolean_selector',feature_true:"+a+",feature_false:"+b+"}",
            "{type:'minecraft:random_selector',features:[{feature:"+a+",chance:0.0},{feature:"+b+",chance:1.0}],default:"+a+"}",
            "{type:'minecraft:simple_random_selector',features:["+a+","+b+","+a+"]}",
            "{type:'minecraft:weighted_random_selector',features:[{data:"+a+",weight:0},{data:"+b+",weight:5}]}",
            "{type:'minecraft:sequence',features:["+a+","+b+","+a+"]}",
            "{type:'minecraft:overlay',features:["+a+","+b+","+a+"]}"
        );
        var cases = new JsonArray();
        var nested = new JsonArray();
        for (var config : configs) {
            var placed = new JsonObject();
            placed.add("feature", JsonParser.parseString(config));
            placed.add("placement", new JsonArray());
            nested.add(placed.deepCopy());
            capture(registries, placed, cases);
        }
        var inline = new JsonObject();
        inline.add("feature", net.minecraft.world.level.levelgen.feature.Feature.DIRECT_CODEC
            .encodeStart(FeaturesGolden.ops(registries), features.getValue(Identifier.withDefaultNamespace("flower_default"))).getOrThrow());
        inline.add("placement", new JsonArray());
        nested.add(inline);
        var sequence = new JsonObject();
        sequence.addProperty("type", "minecraft:sequence");
        sequence.add("features", nested);
        var placed = new JsonObject();
        placed.add("feature", sequence);
        placed.add("placement", new JsonArray());
        capture(registries, placed, cases);
        result.add("composites", cases);
        var mushrooms = new JsonArray();
        for (var id : List.of("huge_brown_mushroom", "huge_red_mushroom")) {
            var config = net.minecraft.world.level.levelgen.feature.Feature.DIRECT_CODEC.encodeStart(
                FeaturesGolden.ops(registries), features.getValue(Identifier.withDefaultNamespace(id))).getOrThrow().getAsJsonObject();
            config.remove("foliage_radius");
            mushroom(registries, config, mushrooms);
            for (int radius : new int[]{-3, 0, 2, 7, Integer.MAX_VALUE}) {
                config.addProperty("foliage_radius", radius);
                mushroom(registries, config, mushrooms);
            }
            config.addProperty("type", "minecraft:no_op");
            mushroom(registries, config, mushrooms);
        }
        result.add("mushrooms", mushrooms);
        WorldgenGolden.write(out.resolve("bonemeal_metadata.json.gz"), result);
    }
    static void mushroom(RegistryAccess.Frozen registries, JsonObject config, JsonArray cases) {
        var feature = net.minecraft.world.level.levelgen.feature.Feature.DIRECT_CODEC.parse(
            FeaturesGolden.ops(registries), config).getOrThrow();
        var item = new JsonObject();
        item.add("config", config.deepCopy());
        if (feature instanceof net.minecraft.world.level.levelgen.feature.AbstractHugeMushroomFeature mushroom) {
            item.addProperty("radius", mushroom.foliageRadius());
        } else {
            item.add("radius", JsonNull.INSTANCE);
        }
        cases.add(item);
    }
    static void capture(RegistryAccess.Frozen registries, JsonObject config, JsonArray cases) {
        var feature = PlacedFeature.DIRECT_CODEC.parse(FeaturesGolden.ops(registries), config).getOrThrow();
        var item = new JsonObject();
        item.add("config", config);
        var traversal = new JsonArray();
        var candidates = new JsonArray();
        feature.getFeatures().forEach(holder -> {
            traversal.add(holder.unwrapKey().map(key -> new JsonPrimitive(key.identifier().toString())).orElse(null));
            if (holder.is(FeatureTags.CAN_SPAWN_FROM_BONE_MEAL)) {
                candidates.add(holder.unwrapKey().orElseThrow().identifier().toString());
            }
        });
        item.add("traversal", traversal);
        item.add("candidates", candidates);
        cases.add(item);
    }
}
