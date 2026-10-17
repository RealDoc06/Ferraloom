import com.google.gson.*;
import java.lang.reflect.Proxy;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.biome.Biomes;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.levelgen.blockpredicates.BlockPredicate;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Tests the real predicates against deterministic reads and entity obstruction. */
public final class PredicateGolden {
    static void generate(RegistryAccess.Frozen registries, Path out) throws Exception {
        var delegate=PlacementGolden.level(registries);
        var plains=registries.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
        var level=(LevelAccessor)Proxy.newProxyInstance(PredicateGolden.class.getClassLoader(),new Class<?>[]{LevelAccessor.class},(object,method,args)->switch(method.getName()) {
            case "getBiome" -> plains;
            case "isUnobstructed" -> (((int)((VoxelShape)args[1]).bounds().minX)&1)==0;
            default -> method.invoke(delegate,args);
        });
        var cases=new JsonArray();
        for(String config:List.of(
            "{type:'true'}",
            "{type:'all_of',predicates:[]}",
            "{type:'any_of',predicates:[]}",
            "{type:'all_of',predicates:[{type:'solid'},{type:'not',predicate:{type:'replaceable'}}]}",
            "{type:'any_of',predicates:[{type:'matching_blocks',blocks:'stone'},{type:'matching_fluids',fluids:'#water'}]}",
            "{type:'not',predicate:{type:'matching_blocks',blocks:'air'}}",
            "{type:'matching_blocks',blocks:['stone','water'],offset:[0,-1,0]}",
            "{type:'matching_block_tag',tag:'base_stone_overworld'}",
            "{type:'matching_fluids',fluids:'#water'}",
            "{type:'matching_fluids',fluids:['empty','water'],offset:[0,1,0]}",
            "{type:'matching_biomes',biomes:'plains'}",
            "{type:'matching_biomes',biomes:'#is_ocean'}",
            "{type:'replaceable',offset:[0,1,0]}",
            "{type:'solid'}",
            "{type:'inside_world_bounds',offset:[0,1,0]}",
            "{type:'has_sturdy_face',direction:'up'}",
            "{type:'has_sturdy_face',direction:'east',offset:[0,-1,0]}",
            "{type:'would_survive',state:'stone',offset:[0,-1,0]}",
            "{type:'unobstructed',offset:[1,0,0]}",
            "{type:'height_range',min_inclusive:{above_bottom:1},max_inclusive:{below_top:1}}",
            "{type:'height_range',min_inclusive:{relative_to_sea_level:-1},max_inclusive:{relative_to_sea_level:1}}",
            "{type:'volume_match',min:[-1,-1,-1],max:[1,1,1],match:{type:'solid'}}"
        )) {
            var value=JsonParser.parseString(config);
            var predicate=BlockPredicate.CODEC.parse(FeaturesGolden.ops(registries),value).getOrThrow();
            var item=new JsonObject();item.add("config",value);var samples=new JsonArray();
            for(int[] p:new int[][]{{0,-65,0},{0,-64,0},{0,-11,0},{-1,-3,1},{0,63,0},{2,70,2},{15,319,15},{15,320,15}}) {
                var sample=new JsonObject();sample.add("position",FeaturesGolden.values(p));sample.addProperty("result",predicate.test(level,new BlockPos(p[0],p[1],p[2])));samples.add(sample);
            }
            item.add("samples",samples);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("predicates.json.gz"),cases);
    }
}
