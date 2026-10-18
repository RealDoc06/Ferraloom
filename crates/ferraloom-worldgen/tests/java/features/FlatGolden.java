import com.google.gson.*;
import java.lang.reflect.Proxy;
import java.nio.file.*;
import java.util.*;
import java.util.function.Function;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.dedicated.DedicatedServerProperties;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.blending.Blender;
import net.minecraft.world.level.levelgen.feature.FillLayerFeature;
import net.minecraft.world.level.levelgen.flat.*;

public final class FlatGolden {
    @SuppressWarnings("unchecked")
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var ops=FeaturesGolden.ops(registries);var cases=new JsonArray();var configs=new LinkedHashMap<String,JsonElement>();
        var defaults=FlatLevelGeneratorSettings.getDefault(registries.lookupOrThrow(Registries.BIOME),registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE));
        configs.put("default",FlatLevelGeneratorSettings.CODEC.encodeStart(ops,defaults).getOrThrow());
        for(var preset:registries.lookupOrThrow(Registries.FLAT_LEVEL_GENERATOR_PRESET).listElements().toList())configs.put(preset.key().identifier().toString(),FlatLevelGeneratorSettings.CODEC.encodeStart(ops,preset.value().settings()).getOrThrow());
        configs.put("custom",JsonParser.parseString("{layers:[{height:1,block:'bedrock'},{height:3,block:'stone'},{height:2,block:'water'},{height:1,block:'snow'},{height:2,block:'oak_leaves'},{height:1,block:'air'}],biome:'plains',structure_overrides:[],features:false,lakes:false}"));
        var getterField=ChunkGenerator.class.getDeclaredField("generationSettingsGetter");getterField.setAccessible(true);
        var noise=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        var randomState=RandomState.create(registries.lookupOrThrow(Registries.NOISE),8675309L,noise);
        for(var config:configs.entrySet()) {
            var settings=FlatLevelGeneratorSettings.CODEC.parse(ops,config.getValue()).getOrThrow();var generator=new FlatLevelSource(settings);generator.validate();
            var item=new JsonObject();item.addProperty("name",config.getKey());item.add("config",config.getValue());var layers=new JsonArray();settings.getLayers().forEach(state->layers.add(state==null?JsonNull.INSTANCE:new JsonPrimitive(Block.getId(state))));item.add("layers",layers);
            var generation=((Function<Holder<Biome>,BiomeGenerationSettings>)getterField.get(generator)).apply(settings.getBiome());var steps=new JsonArray();
            for(var step:generation.features()) {var features=new JsonArray();for(var placed:step) {if(placed.unwrapKey().isPresent())features.add(placed.unwrapKey().get().identifier().toString());else {var fill=(FillLayerFeature)placed.value().feature().value();features.add(FeaturesGolden.values(fill.height(),Block.getId(fill.state())));}}steps.add(features);}item.add("steps",steps);
            var heights=new JsonArray();for(int[] bounds:new int[][]{{-64,384},{0,256},{-64,32}}) {var accessor=LevelHeightAccessor.create(bounds[0],bounds[1]);var query=new JsonObject();query.add("bounds",FeaturesGolden.values(bounds));query.addProperty("spawn",generator.getSpawnHeight(accessor));var values=new JsonArray();for(var type:Heightmap.Types.values())values.add(generator.getBaseHeight(37,-21,type,accessor,randomState));query.add("heights",values);var column=new JsonArray();var actual=generator.getBaseColumn(37,-21,accessor,randomState);for(int y=bounds[0]-1;y<=bounds[0]+bounds[1];y++)column.add(Block.getId(actual.getBlock(y)));query.add("column",column);heights.add(query);}item.add("queries",heights);
            var chunks=new JsonArray();for(int[] position:new int[][]{{0,0},{-1,1},{9,-3}}) {
                var accessor=LevelHeightAccessor.create(-64,384);var neighborhood=new HashMap<ChunkPos,ProtoChunk>();
                for(int x=position[0]-1;x<=position[0]+1;x++)for(int z=position[1]-1;z<=position[1]+1;z++) {var chunk=new ProtoChunk(new ChunkPos(x,z),UpgradeData.EMPTY,accessor,PalettedContainerFactory.create(registries),null);chunk.fillBiomesFromNoise((qx,qy,qz)->settings.getBiome());generator.buildTerrain(chunk,Blender.empty(),randomState,null,null,null,Set.of(settings.getBiome())).join();chunk.setPersistedStatus(ChunkStatus.TERRAIN);neighborhood.put(chunk.getPos(),chunk);}
                var center=neighborhood.get(new ChunkPos(position[0],position[1]));var value=new JsonObject();value.add("position",FeaturesGolden.values(position));value.add("terrain",WorldgenGolden.blocks(center));
                if(config.getKey().equals("default")||config.getKey().endsWith("classic_flat")||config.getKey().endsWith("the_void")||config.getKey().equals("custom")) {
                    var level=(WorldGenLevel)Proxy.newProxyInstance(FlatGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(object,method,args)->switch(method.getName()) {
                        case "registryAccess"->registries;case "getMinY"->-64;case "getMinSectionY"->-4;case "getMaxY"->319;case "getSeed"->8675309L;
                        case "getChunk"->neighborhood.get(new ChunkPos((int)args[0],(int)args[1]));
                        case "getBiome"->settings.getBiome();case "getHeight"->384;case "setCurrentlyGenerating"->null;
                        case "getBlockState"->{var p=(BlockPos)args[0];yield neighborhood.get(ChunkPos.containing(p)).getBlockState(p);}
                        case "setBlock"->{var p=(BlockPos)args[0];neighborhood.get(ChunkPos.containing(p)).setBlockState(p,(net.minecraft.world.level.block.state.BlockState)args[1]);yield true;}
                        default->throw new UnsupportedOperationException(method.toString());
                    });
                    generator.applyBiomeDecoration(level,center,new StructureManager(level,new WorldOptions(8675309L,false,false),null));value.add("features",WorldgenGolden.blocks(center));
                }
                chunks.add(value);
            }
            item.add("chunks",chunks);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("flat_chunks.json.gz"),cases);
        var presets=new JsonArray();for(var holder:registries.lookupOrThrow(Registries.WORLD_PRESET).listElements().toList()) {var item=new JsonObject();item.addProperty("name",holder.key().identifier().toString());item.add("definition",net.minecraft.world.level.levelgen.presets.WorldPreset.DIRECT_CODEC.encodeStart(ops,holder.value()).getOrThrow());presets.add(item);}WorldgenGolden.write(out.resolve("world_presets.json.gz"),presets);
        var mappings=new JsonArray();for(String name:List.of("normal","NORMAL","flat","flat_all_dimensions","large_biomes","largebiomes","amplified","single_biome_surface","debug_all_block_states","debug","default","unknown","Bad!Type")) {var props=new Properties();props.setProperty("level-type",name);var dimensions=new DedicatedServerProperties(props).createDimensions(registries);var item=new JsonObject();item.addProperty("level_type",name);item.add("definition",WorldDimensions.CODEC.codec().encodeStart(ops,dimensions).getOrThrow());mappings.add(item);}WorldgenGolden.write(out.resolve("level_types.json.gz"),mappings);
        var debug=new JsonArray();for(int x=-1;x<=385;x+=2) {var column=new JsonArray();for(int z=-1;z<=385;z+=2)column.add(Block.getId(DebugLevelSource.getBlockStateFor(x,z)));debug.add(column);}WorldgenGolden.write(out.resolve("debug_grid.json.gz"),debug);
    }
}
