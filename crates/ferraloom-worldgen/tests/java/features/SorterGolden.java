import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.level.biome.FeatureSorter;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;
public final class SorterGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        var entries=registries.lookupOrThrow(Registries.PLACED_FEATURE).listElements().sorted(Comparator.comparing(e->e.key().identifier())).limit(6).toList();
        var sources=List.of(
            "[[[0,1],[2]],[[0,3],[2,4]]]",
            "[[[0],[1]],[[2],[3]]]",
            "[[[0,1]],[[1,0]]]",
            "[[[0],[0,1]],[[0],[1,2]]]",
            "[[[],[0,1]],[[2],[],[3,4,5]]]"
        );
        for(String source:sources) {
            var input=JsonParser.parseString(source);var data=new ArrayList<List<HolderSet<PlacedFeature>>>();
            for(var biome:input.getAsJsonArray()) { var steps=new ArrayList<HolderSet<PlacedFeature>>();for(var step:biome.getAsJsonArray()) { var holders=new ArrayList<Holder<PlacedFeature>>();for(var f:step.getAsJsonArray())holders.add(entries.get(f.getAsInt()));steps.add(HolderSet.direct(holders)); }data.add(steps); }
            var item=new JsonObject();item.add("sources",input);var output=new JsonArray();
            try { var sorted=FeatureSorter.buildFeaturesPerStep(data,java.util.function.Function.identity(),false);for(var step:sorted) {var fs=new JsonArray();for(var f:step.features()) {int index=-1;for(int i=0;i<entries.size();i++)if(entries.get(i).value()==f)index=i;fs.add(index);}output.add(fs);}item.add("steps",output); }
            catch(IllegalStateException error) { item.addProperty("cycle",true); }
            cases.add(item);
        }
        WorldgenGolden.write(out.resolve("feature_sorter.json.gz"),cases);
    }
}
