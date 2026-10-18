import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import com.mojang.serialization.MapCodec;
import java.lang.reflect.Proxy;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.*;
import net.minecraft.world.level.levelgen.placement.*;

/** A deterministic WorldGenLevel view exercises vanilla placement without a server. */
public final class PlacementGolden {
    static BlockState block(BlockPos p) {
        int y=p.getY();
        if(p.getX()==2 && y==70 && p.getZ()==2)return Blocks.OAK_STAIRS.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED,true).setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.HORIZONTAL_FACING,Direction.EAST);
        if(y < -64 || y > 319)return Blocks.VOID_AIR.defaultBlockState();
        if(y == -64)return Blocks.BEDROCK.defaultBlockState();
        if(y == 63 && (p.getX()&3)==0)return Blocks.WATER.defaultBlockState();
        if(y >= 0 && y <= 63 || y < -10)return Blocks.STONE.defaultBlockState();
        return Blocks.AIR.defaultBlockState();
    }
    static WorldGenLevel level(RegistryAccess.Frozen registries) {
        return (WorldGenLevel)Proxy.newProxyInstance(PlacementGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(object,method,args)-> switch(method.getName()) {
            case "getBlockState" -> block((BlockPos)args[0]);
            case "getFluidState" -> block((BlockPos)args[0]).getFluidState();
            case "getMinY" -> -64;
            case "getMaxY" -> 319;
            case "getHeight" -> args == null || args.length == 0 ? 384 : (args[0] == Heightmap.Types.OCEAN_FLOOR || args[0] == Heightmap.Types.OCEAN_FLOOR_WG) && (((int)args[1]&3)==0) ? 63 : 64;
            case "getSeaLevel" -> 63;
            case "getSeed" -> 8675309L;
            case "isOutsideBuildHeight" -> (int)args[0] < -64 || (int)args[0] >= 320;
            case "isInsideBuildHeight" -> ((BlockPos)args[0]).getY() >= -64 && ((BlockPos)args[0]).getY() < 320;
            case "registryAccess" -> registries;
            case "toString" -> "FeatureParityLevel";
            default -> throw new UnsupportedOperationException(method.toString());
        });
    }
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(net.minecraft.resources.Identifier.withDefaultNamespace("overworld"));
        var generator=new NoiseBasedChunkGenerator(WorldgenGolden.source(registries,"overworld"),Holder.direct(settings));
        var level=level(registries);
        var configs=List.of(
            "[]",
            "[{type:'count',count:{type:'uniform',min_inclusive:2,max_inclusive:5}},{type:'in_square'},{type:'heightmap',heightmap:'WORLD_SURFACE'}]",
            "[{type:'count',count:3},{type:'in_square'},{type:'rarity_filter',chance:3},{type:'height_range',height:{type:'uniform',min_inclusive:{above_bottom:8},max_inclusive:{relative_to_sea_level:40}}}]",
            "[{type:'cuboid',xz_size:2,y_size:3,include_edges:false,include_interior:false}]",
            "[{type:'cuboid',xz_size:2,y_size:3,include_edges:true,include_interior:true},{type:'random_chance',chance:0.3}]",
            "[{type:'offset',x:{type:'uniform',min_inclusive:-3,max_inclusive:4},y:5,z:{type:'trapezoid',min:-7,max:7,plateau:0}}]",
            "[{type:'fixed_placement',positions:[[0,4,0],[15,8,15],[16,4,0],[-1,0,-1],[-112,9,176]]}]",
            "[{type:'randomly_selected',placements:[{type:'count',count:3},{type:'in_square'}]}]",
            "[{type:'noise_based_count',noise_to_count_ratio:8,noise_factor:80.0,noise_offset:0.5},{type:'offset',x:1,y:0,z:0}]",
            "[{type:'noise_threshold_count',noise_level:0.1,below_noise:2,above_noise:5}]",
            "[{type:'count_on_every_layer',count:{type:'uniform',min_inclusive:1,max_inclusive:3}}]",
            "[{type:'environment_scan',direction_of_search:'down',max_steps:10,target_condition:{type:'matching_blocks',blocks:'stone'},allowed_search_condition:{type:'replaceable'}}]",
            "[{type:'environment_scan',direction_of_search:'up',max_steps:32,target_condition:{type:'matching_blocks',blocks:'stone'}}]",
            "[{type:'surface_relative_threshold_filter',heightmap:'WORLD_SURFACE',min_inclusive:-20,max_inclusive:20}]",
            "[{type:'surface_water_depth_filter',max_water_depth:0}]",
            "[{type:'heightmap',heightmap:'MOTION_BLOCKING_NO_LEAVES'}]",
            "[{type:'block_predicate_filter',predicate:{type:'all_of',predicates:[{type:'not',predicate:{type:'matching_blocks',blocks:['stone','bedrock']}},{type:'inside_world_bounds'}]}}]"
        );
        for(String config:configs)for(long seed:new long[]{0,1,-1,8675309})for(int[] origin:new int[][]{{0,70,0},{-112,-3,176}}) {
            var value=JsonParser.parseString(config);var modifiers=new ArrayList<PlacementModifier>();
            for(var modifier:value.getAsJsonArray())modifiers.add(PlacementModifier.CODEC.parse(FeaturesGolden.ops(registries),modifier).getOrThrow());
            var positions=new JsonArray();
            Feature feature=new Feature() {
                public MapCodec<? extends Feature> codec() { return NoOpFeature.CODEC; }
                public boolean place(WorldGenLevel level,ChunkGenerator gen,RandomSource random,BlockPos p) { var item=new JsonObject();item.add("position",FeaturesGolden.values(p.getX(),p.getY(),p.getZ()));item.addProperty("random",random.nextLong());positions.add(item);return (p.getY()&1)==0; }
            };
            var placed=new PlacedFeature(Holder.direct(feature),modifiers);
            var random=new WorldgenRandom(new XoroshiroRandomSource(seed));
            boolean success=new FeaturePlacer(level,generator).place(placed,random,new BlockPos(origin[0],origin[1],origin[2]));
            var item=new JsonObject();item.add("placement",value);item.addProperty("seed",seed);item.add("origin",FeaturesGolden.values(origin));item.add("positions",positions);item.addProperty("success",success);item.addProperty("next",random.nextLong());cases.add(item);
        }
        WorldgenGolden.write(out.resolve("placement.json.gz"),cases);
    }
}
