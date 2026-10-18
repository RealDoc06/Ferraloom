import com.google.gson.*;
import com.mojang.serialization.MapCodec;
import java.lang.reflect.Proxy;
import java.nio.file.*;
import java.util.*;
import java.util.function.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.*;
import net.minecraft.world.level.levelgen.placement.*;

/** Calls ChunkGenerator.applyBiomeDecoration itself with structures disabled. */
public final class DecorationGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();var biomes=registries.lookupOrThrow(Registries.BIOME);
        var plains=biomes.getOrThrow(Biomes.PLAINS);var forest=biomes.getOrThrow(Biomes.FOREST);
        var source=new CheckerboardColumnBiomeSource(HolderSet.direct(plains,forest),0);
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        for(long seed:new long[]{0,1,-1,8675309})for(int[] p:new int[][]{{0,0},{-7,11}})for(boolean mixed:new boolean[]{false,true}) {
            var output=new JsonArray();var features=new ArrayList<Holder<PlacedFeature>>();
            for(int i=0;i<6;i++) {final int id=i;Feature feature=new Feature() {
                public MapCodec<? extends Feature> codec() { return NoOpFeature.CODEC; }
                public boolean place(WorldGenLevel level,ChunkGenerator gen,net.minecraft.util.RandomSource random,BlockPos pos) {var record=new JsonObject();record.addProperty("feature",id);record.add("position",FeaturesGolden.values(pos.getX(),pos.getY(),pos.getZ()));record.addProperty("random",random.nextLong());output.add(record);return true;}
            };features.add(Holder.direct(new PlacedFeature(Holder.direct(feature),List.of(CountPlacement.of(2),BiomeFilter.biome()))));}
            var a=new BiomeGenerationSettings.PlainBuilder().addFeature(0,features.get(0)).addFeature(0,features.get(1)).addFeature(6,features.get(2)).addFeature(9,features.get(3)).build();
            var b=new BiomeGenerationSettings.PlainBuilder().addFeature(0,features.get(0)).addFeature(0,features.get(4)).addFeature(6,features.get(2)).addFeature(6,features.get(5)).addFeature(9,features.get(3)).build();
            Function<Holder<Biome>,BiomeGenerationSettings> getter=biome->biome==plains?a:b;
            var generator=new NoiseBasedChunkGenerator(source,settings);
            var getterField=ChunkGenerator.class.getDeclaredField("generationSettingsGetter");getterField.setAccessible(true);getterField.set(generator,getter);
            var featureField=ChunkGenerator.class.getDeclaredField("featuresPerStep");featureField.setAccessible(true);featureField.set(generator,(Supplier<List<FeatureSorter.StepFeatureData>>)()->FeatureSorter.buildFeaturesPerStep(List.copyOf(source.possibleBiomes()),h->getter.apply(h).features(),true));
            var chunks=new HashMap<ChunkPos,ProtoChunk>();
            for(int x=p[0]-1;x<=p[0]+1;x++)for(int z=p[1]-1;z<=p[1]+1;z++) {var pos=new ChunkPos(x,z);var chunk=new ProtoChunk(pos,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),null);var biome=mixed&&((x+z)&1)!=0?forest:plains;chunk.fillBiomesFromNoise((qx,qy,qz)->biome);chunk.setPersistedStatus(net.minecraft.world.level.chunk.status.ChunkStatus.TERRAIN);chunks.put(pos,chunk);}
            // Include the one-chunk biome zoom halo used by BiomeFilter.
            var manager=new BiomeManager((qx,qy,qz)-> {var c=chunks.get(new ChunkPos(qx>>2,qz>>2));return c==null?plains:c.getNoiseBiome(qx,qy,qz);},BiomeManager.obfuscateSeed(seed));
            var level=(WorldGenLevel)Proxy.newProxyInstance(DecorationGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(obj,method,args)->switch(method.getName()) {
                case "registryAccess" -> registries;
                case "getMinY" -> -64;
                case "getMinSectionY" -> -4;
                case "getSeed" -> seed;
                case "getChunk" -> chunks.get(new ChunkPos((int)args[0],(int)args[1]));
                case "getBiome" -> manager.getBiome((BlockPos)args[0]);
                case "getHeight" -> 384;
                case "setCurrentlyGenerating" -> null;
                case "toString" -> "DecorationParityLevel";
                default -> throw new UnsupportedOperationException(method.toString());
            });
            generator.applyBiomeDecoration(level,chunks.get(new ChunkPos(p[0],p[1])),new StructureManager(level,new WorldOptions(seed,false,false),null));
            var item=new JsonObject();item.addProperty("seed",seed);item.add("chunk",FeaturesGolden.values(p));item.addProperty("mixed",mixed);item.add("output",output);var possible=new JsonArray();source.possibleBiomes().forEach(h->possible.add(h==plains?"minecraft:plains":"minecraft:forest"));item.add("possible",possible);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("decoration.json.gz"),cases);
    }
}
