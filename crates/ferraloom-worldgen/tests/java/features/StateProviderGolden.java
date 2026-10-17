import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.stateproviders.*;

public final class StateProviderGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();var level=PlacementGolden.level(registries);
        var configs=List.of(
            "{id:'oak_log',properties:{axis:'x'}}",
            "{type:'simple',state:'stone'}",
            "{type:'weighted',entries:[{data:'stone',weight:0},{data:'oak_log',weight:2},{data:'gold_block',weight:3}]}",
            "{type:'random_block',blocks:['stone','oak_log','gold_block']}",
            "{type:'random_block',blocks:[]}",
            "{type:'copy_properties',source:{id:'spruce_stairs'}}",
            "{type:'randomized_int',source:{id:'wheat'},property:'age',values:{type:'uniform',min_inclusive:0,max_inclusive:7}}",
            "{type:'randomized_int',source:{id:'oak_log'},property:'axis',values:1}",
            "{type:'rotated',state:{id:'oak_log'}}",
            "{type:'rotated',state:{id:'oak_stairs'}}",
            "{type:'rotated',state:{id:'oak_log'},direction:'east'}",
            "{type:'rule_based',rules:[{if_true:{type:'matching_blocks',blocks:'stone'},then:{id:'dirt'}},{if_true:{type:'true'},then:{type:'random_block',blocks:[]}}]}",
            "{type:'rule_based',fallback:{id:'gold_block'},rules:[]}",
            "{type:'noise',seed:2345,noise:{base_octave:-3,octave_count:3},scale:0.15,states:['stone','oak_log','gold_block']}",
            "{type:'noise_threshold',seed:2345,noise:{base_octave:-3,octave_count:3},scale:0.15,threshold:0.0,high_chance:0.4,default_state:'stone',low_states:['oak_log','dirt'],high_states:['gold_block']}",
            "{type:'dual_noise',seed:2345,noise:{base_octave:-3,octave_count:3},scale:0.15,states:['stone','oak_log','gold_block'],variety:{min_inclusive:1,max_inclusive:3},slow_noise:{base_octave:-5},slow_scale:0.01}"
        );
        var points=new int[][]{{0,70,0},{2,70,2},{-12,-3,16},{15,63,15},{-17,-60,-17}};
        var ops=FeaturesGolden.ops(registries);
        for(String config:configs)for(long seed:new long[]{0,1,-1,8675309}) {
            var value=JsonParser.parseString(config);var provider=BlockStateProvider.DIRECT_CODEC.parse(ops,value).getOrThrow();
            var random=new WorldgenRandom(new XoroshiroRandomSource(seed));var samples=new JsonArray();var optional=new JsonArray();
            for(int i=0;i<32;i++) { int[] p=points[i%points.length];var pos=new BlockPos(p[0],p[1],p[2]);var state=provider.getOptionalState(level,random,pos);optional.add(state==null?JsonNull.INSTANCE:new JsonPrimitive(Block.getId(state)));samples.add(Block.getId(provider.getState(level,random,pos))); }
            var item=new JsonObject();item.add("config",value);item.addProperty("seed",seed);item.add("optional",optional);item.add("samples",samples);item.addProperty("next",random.nextLong());cases.add(item);
        }
        WorldgenGolden.write(out.resolve("state_providers.json.gz"),cases);
        var rotations=new JsonArray();
        for(var direction:Direction.values()) {
            var values=new JsonArray();var random=new WorldgenRandom(new XoroshiroRandomSource(0));
            for(var state:Block.BLOCK_STATE_REGISTRY) {
                var provider=new RotatedBlockProvider(Holder.direct(new SimpleStateProvider(state)),Optional.of(direction));
                values.add(Block.getId(provider.getState(level,random,BlockPos.ZERO)));
            }
            rotations.add(values);
        }
        WorldgenGolden.write(out.resolve("rotated_state_providers.json.gz"),rotations);
    }
}
