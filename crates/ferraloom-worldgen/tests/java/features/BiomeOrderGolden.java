import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.placement.*;
public final class BiomeOrderGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        var configs=List.of(
            "{type:'multi_noise',preset:'minecraft:overworld'}",
            "{type:'multi_noise',preset:'minecraft:nether'}",
            "{type:'the_end'}",
            "{type:'fixed',biome:'minecraft:forest'}",
            "{type:'checkerboard',biomes:['minecraft:forest','minecraft:plains','minecraft:forest','minecraft:desert']}",
            "{type:'checkerboard',scale:62,biomes:['minecraft:plains','minecraft:desert']}",
            "{type:'checkerboard',biomes:[]}"
        );
        for(String config:configs) {
            var value=JsonParser.parseString(config);var source=BiomeSource.CODEC.parse(FeaturesGolden.ops(registries),value).getOrThrow();
            String dimension=config.contains("nether")?"nether":config.contains("the_end")?"end":"overworld";
            var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(net.minecraft.resources.Identifier.withDefaultNamespace(dimension));
            var random=RandomState.create(registries.lookupOrThrow(Registries.NOISE),8675309,settings);var resolver=source.createUncachedResolver(random);
            var item=new JsonObject();item.add("config",value);item.addProperty("dimension",dimension);var possible=new JsonArray();source.possibleBiomes().forEach(h->possible.add(h.unwrapKey().orElseThrow().identifier().toString()));item.add("possible",possible);
            var samples=new JsonArray();if(!source.possibleBiomes().isEmpty())for(int[] p:new int[][]{{0,0,0},{-1,-64,-1},{7,25,-31},{Integer.MAX_VALUE,0,Integer.MAX_VALUE},{Integer.MIN_VALUE,1,-13456}})samples.add(resolver.getNoiseBiome(p[0],p[1],p[2]).unwrapKey().orElseThrow().identifier().toString());item.add("samples",samples);
            var sorted=new JsonArray();for(var step:FeatureSorter.buildFeaturesPerStep(List.copyOf(source.possibleBiomes()),h->h.value().getGenerationSettings().features(),true)) {var entries=new JsonArray();for(var feature:step.features())entries.add(registries.lookupOrThrow(Registries.PLACED_FEATURE).getResourceKey(feature).orElseThrow().identifier().toString());sorted.add(entries);}item.add("features",sorted);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("biome_order.json.gz"),cases);
    }
}
