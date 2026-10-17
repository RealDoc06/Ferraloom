import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.util.valueproviders.*;
import net.minecraft.world.level.levelgen.blockpredicates.BlockPredicate;
import net.minecraft.world.level.levelgen.heightproviders.HeightProvider;
import net.minecraft.world.level.levelgen.placement.PlacementModifier;
import net.minecraft.world.level.levelgen.feature.stateproviders.BlockStateProvider;
public final class CodecGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        var configs=List.of(
            new String[]{"int","1.9"},new String[]{"int","4294967297"},
            new String[]{"int","{type:'uniform',min_inclusive:2,max_inclusive:1}"},
            new String[]{"int","{type:'clamped',source:1,min_inclusive:5,max_inclusive:8}"},
            new String[]{"int","{type:'clamped_normal',mean:0,deviation:-1,min_inclusive:0,max_inclusive:3}"},
            new String[]{"int","{type:'weighted_list',distribution:[]}"},
            new String[]{"int","{type:'weighted_list',distribution:[{data:1,weight:0}]}"},
            new String[]{"int","{type:'weighted_list',distribution:[{data:1,weight:2147483647},{data:2,weight:1}]}"},
            new String[]{"int","{type:'trapezoid',min:0,max:3,plateau:-2}"},
            new String[]{"float","{type:'uniform',min_inclusive:2,max_exclusive:2}"},
            new String[]{"float","{type:'clamped_normal',mean:0,deviation:-1,min:2,max:1}"},
            new String[]{"float","{type:'trapezoid',min:0,max:3,plateau:-2}"},
            new String[]{"height","{type:'biased_to_bottom',min_inclusive:{absolute:0},max_inclusive:{absolute:10},inner:0}"},
            new String[]{"height","{type:'uniform',min_inclusive:{absolute:10},max_inclusive:{absolute:0}}"},
            new String[]{"height","{type:'trapezoid',min_inclusive:{absolute:0},max_inclusive:{absolute:3},plateau:100}"},
            new String[]{"height","{absolute:2,above_bottom:3}"},
            new String[]{"predicate","{type:'replaceable',offset:[16,-16,0]}"},
            new String[]{"predicate","{type:'replaceable',offset:[17,0,0]}"},
            new String[]{"predicate","{type:'replaceable',offset:[-2147483648,0,0]}"},
            new String[]{"predicate","{type:'unobstructed',offset:[2147483647,0,0]}"},
            new String[]{"predicate","{type:'volume_match',min:[2,0,0],max:[1,0,0],match:{type:'true'}}"},
            new String[]{"predicate","{type:'matching_blocks',blocks:[]}"},
            new String[]{"placement","{type:'count',count:4097}"},
            new String[]{"placement","{type:'count_on_every_layer',count:257}"},
            new String[]{"placement","{type:'offset',x:17,y:0,z:0}"},
            new String[]{"placement","{type:'randomly_selected',placements:[]}"},
            new String[]{"placement","{type:'environment_scan',direction_of_search:'north',target_condition:{type:'true'},max_steps:4}"},
            new String[]{"placement","{type:'surface_relative_threshold_filter',heightmap:'WORLD_SURFACE',min_inclusive:20,max_inclusive:0}"},
            new String[]{"state","{id:'stone',type:'missing'}"},
            new String[]{"state","{id:'stone',type:'simple',state:'dirt'}"},
            new String[]{"state","{id:'missing',type:'simple',state:'dirt'}"},
            new String[]{"state","{id:'stone',type:'simple'}"},
            new String[]{"state","{type:'dual_noise',seed:1,noise:{base_octave:0},scale:1,states:['stone'],variety:2,slow_noise:{base_octave:0},slow_scale:1}"},
            new String[]{"state","{type:'dual_noise',seed:1,noise:{base_octave:0},scale:1,states:['stone'],variety:[2,3],slow_noise:{base_octave:0},slow_scale:1}"}
        );
        for(var config:configs) {var value=JsonParser.parseString(config[1]);boolean valid;try {var result=switch(config[0]) {case "int"->IntProviders.CODEC.parse(JsonOps.INSTANCE,value);case "float"->FloatProviders.CODEC.parse(JsonOps.INSTANCE,value);case "height"->HeightProvider.CODEC.parse(JsonOps.INSTANCE,value);case "predicate"->BlockPredicate.CODEC.parse(FeaturesGolden.ops(registries),value);case "placement"->PlacementModifier.CODEC.parse(FeaturesGolden.ops(registries),value);case "state"->BlockStateProvider.DIRECT_CODEC.parse(FeaturesGolden.ops(registries),value);default->throw new AssertionError();};valid=result.isSuccess();}catch(RuntimeException error){valid=false;}var item=new JsonObject();item.addProperty("kind",config[0]);item.add("config",value);item.addProperty("valid",valid);cases.add(item);}
        WorldgenGolden.write(out.resolve("feature_codecs.json.gz"),cases);
    }
}
