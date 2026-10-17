import com.google.gson.*;
import java.nio.file.*;
import java.io.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.network.chat.Component;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.densityfunction.*;

/** Reference scenarios against real jar resources, with no server or world IO. */
public final class WorldgenGolden {
    static final Gson JSON = new GsonBuilder().disableHtmlEscaping().create();
    static final long[] SEEDS={0,-1,8675309};
    static final int[][] POINTS={{0,-64,0},{-1,7,-17},{-17,-63,31},{12345,93,-54321},{-30000000,319,29999999},{33554431,-2032,-33554432},{-4097,0,8191}};
    static JsonArray array(long... values) { var result=new JsonArray();for(long v:values)result.add(v);return result; }
    static JsonArray point(int[] p) {return array(p[0],p[1],p[2]);}
    static void write(Path path, JsonElement data) throws Exception {
        try(var out=new GZIPOutputStream(Files.newOutputStream(path))) {out.write(JSON.toJson(data).getBytes(java.nio.charset.StandardCharsets.UTF_8));}
    }
    static RegistryAccess.Frozen load() {
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        var manager=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(manager,builtins).forEach(Registry.PendingTags::apply);
        return RegistryDataLoader.load(manager,builtins.registries().<HolderLookup.RegistryLookup<?>>map(e -> e.value()).toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
    }
    static void presets(Path directory) throws Exception {
        var out=new JsonObject();
        for(var entry:MultiNoiseBiomeSourceParameterList.knownPresets().entrySet().stream().sorted(Comparator.comparing(e->e.getKey().id())).toList()) {
            var values=new JsonArray();
            for(var pair:entry.getValue().values()) {
                var p=pair.getFirst();var value=new JsonArray();
                for(var range:List.of(p.temperature(),p.humidity(),p.continentalness(),p.erosion(),p.depth(),p.weirdness())) value.add(array(range.min(),range.max()));
                value.add(array(p.offset(),p.offset()));value.add(pair.getSecond().identifier().toString());values.add(value);
            }
            out.add(entry.getKey().id().toString(),values);
        }
        write(directory.resolve("../../src/biome_presets.json.gz"),out);
    }
    static void densities(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var definitions=registries.lookupOrThrow(Registries.DENSITY_FUNCTION);
        var noises=registries.lookupOrThrow(Registries.NOISE);
        var settingsRegistry=registries.lookupOrThrow(Registries.NOISE_SETTINGS);
        var out=new JsonArray();
        for(long seed:SEEDS) for(String dimension:List.of("overworld","nether","end")) {
            var settings=settingsRegistry.getValue(Identifier.withDefaultNamespace(dimension));
            var state=RandomState.create(noises,seed,settings);
            var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("dimension",dimension);
            var functions=new JsonObject();
            for(var entry:definitions.listElements().sorted(Comparator.comparing(h->h.key().identifier())).toList()) {
                var function=entry.value();var samples=new JsonArray();
                for(int[] p:POINTS) samples.add(Integer.toUnsignedLong(Float.floatToRawIntBits(state.getSampler(function).sampleValue(SamplerContext.EMPTY_UNCACHED,p[0],p[1],p[2]))));
                functions.add(entry.key().identifier().toString(),samples);
            }
            item.add("functions",functions);
            var router=settings.noiseRouter();var volumes=new JsonObject();
            var volume=new DensityVolume(3,5,2,-17,-63,31,4,3,7);
            var context=SamplerContext.builder().enableCaches().build();
            for(var entry:List.of(Map.entry("temperature",router.temperature()),Map.entry("vegetation",router.vegetation()),Map.entry("continents",router.continents()),Map.entry("erosion",router.erosion()),Map.entry("depth",router.depth()),Map.entry("ridges",router.ridges()),Map.entry("chunk_surface_level",router.chunkSurfaceLevel()),Map.entry("final_density",router.finalDensity()))) {
                var values=new JsonArray();var sampler=state.getSampler(entry.getValue());
                try(var buffer=context.acquireBuffer(volume)) {
                    sampler.sampleVolume(context,buffer,volume);
                    for(int i=0;i<volume.size();i++)values.add(Integer.toUnsignedLong(Float.floatToRawIntBits(buffer.get(i))));
                }
                volumes.add(entry.getKey(),values);
            }
            item.add("volumes",volumes);out.add(item);
        }
        write(directory.resolve("density.json.gz"),out);
    }
    static BiomeSource source(RegistryAccess.Frozen registries,String dimension) {
        var biomes=registries.lookupOrThrow(Registries.BIOME);
        if(dimension.equals("end")) return TheEndBiomeSource.create(biomes);
        var preset=dimension.equals("nether")?MultiNoiseBiomeSourceParameterList.Preset.NETHER:MultiNoiseBiomeSourceParameterList.Preset.OVERWORLD;
        return MultiNoiseBiomeSource.createFromPreset(Holder.direct(new MultiNoiseBiomeSourceParameterList(preset,biomes)));
    }
    static List<Holder<Biome>> biomeGrid(BiomeSource source,RandomState state,SamplerContext context,int cx,int cz,int minY,int height) {
        int sy=height/4;
        var resolver=source.createResolverForChunk(state.createClimateSampler(context),cx*4,minY/4,cz*4,4,sy,4);
        List<Holder<Biome>> result=new ArrayList<>(Collections.nCopies(sy*16,null));
        for(int section=0;section<height/16;section++) for(int x=0;x<4;x++)for(int y=0;y<4;y++)for(int z=0;z<4;z++) {
            int qy=section*4+y;result.set((z*4+x)*sy+qy,resolver.getNoiseBiome(cx*4+x,minY/4+qy,cz*4+z));
        }
        return result;
    }
    static String biomeHash(List<Holder<Biome>> values) throws Exception {
        var digest=java.security.MessageDigest.getInstance("SHA-256");
        for(var biome:values)digest.update((biome.unwrapKey().orElseThrow().identifier()+"\n").getBytes(java.nio.charset.StandardCharsets.UTF_8));
        return HexFormat.of().formatHex(digest.digest());
    }
    static void biomes(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var out=new JsonArray();
        for(long seed:SEEDS) for(String dimension:List.of("overworld","nether","end")) {
            var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace(dimension));
            var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings);
            var source=source(registries,dimension);var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("dimension",dimension);
            var resolver=source.createUncachedResolver(state);var samples=new JsonArray();
            for(int[] p:POINTS)samples.add(resolver.getNoiseBiome(p[0],p[1],p[2]).unwrapKey().orElseThrow().identifier().toString());
            item.add("samples",samples);
            item.addProperty("grid",biomeHash(biomeGrid(source,state,SamplerContext.builder().enableCaches().build(),-7,11,settings.noiseSettings().minY(),settings.noiseSettings().height())));
            out.add(item);
        }
        write(directory.resolve("biomes.json.gz"),out);
    }
    static JsonObject blocks(net.minecraft.world.level.chunk.ProtoChunk chunk) throws Exception {
        var out=new JsonObject();var digest=java.security.MessageDigest.getInstance("SHA-256");
        for(int z=0;z<16;z++)for(int x=0;x<16;x++)for(int y=chunk.getMinY();y<=chunk.getMaxY();y++) {
            int id=net.minecraft.world.level.block.Block.getId(chunk.getBlockState(new BlockPos(chunk.getPos().getMinBlockX()+x,y,chunk.getPos().getMinBlockZ()+z)));
            digest.update((byte)(id>>8));digest.update((byte)id);
        }
        out.addProperty("hash",HexFormat.of().formatHex(digest.digest()));
        var columns=new JsonArray();
        for(int[] local:new int[][]{{0,0},{7,11},{15,15}}) {
            var column=new JsonArray();int previous=-1,count=0;
            for(int y=chunk.getMinY();y<=chunk.getMaxY();y++) {
                int id=net.minecraft.world.level.block.Block.getId(chunk.getBlockState(new BlockPos(chunk.getPos().getMinBlockX()+local[0],y,chunk.getPos().getMinBlockZ()+local[1])));
                if(previous!=id && count>0) {column.add(array(previous,count));count=0;}previous=id;count++;
            }
            column.add(array(previous,count));columns.add(column);
        }
        out.add("columns",columns);
        var surface=new JsonArray();var floor=new JsonArray();
        for(int z=0;z<16;z++)for(int x=0;x<16;x++) {surface.add(chunk.getHeight(Heightmap.Types.WORLD_SURFACE_WG,x,z)+1);floor.add(chunk.getHeight(Heightmap.Types.OCEAN_FLOOR_WG,x,z)+1);}
        out.add("world_surface",surface);out.add("ocean_floor",floor);
        return out;
    }
    static List<int[]> locations(RegistryAccess.Frozen registries,long seed,String dimension) {
        var locations=new ArrayList<int[]>();locations.add(new int[]{0,0});locations.add(new int[]{-7,-13});
        if(!dimension.equals("overworld")) {locations.add(new int[]{-89,73});return locations;}
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace(dimension));
        var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings);var resolver=source(registries,dimension).createCachingResolver(state);
        var needed=new HashSet<>(List.of("minecraft:ocean","minecraft:eroded_badlands","minecraft:frozen_peaks","minecraft:frozen_ocean"));
        for(int radius=0;radius<=256 && !needed.isEmpty();radius+=7) for(int i=-radius;i<=radius && !needed.isEmpty();i+=7) for(int[] p:new int[][]{{i,-radius},{i,radius},{-radius,i},{radius,i}}) {
            var biome=resolver.getNoiseBiome(p[0]*4+2,24,p[1]*4+2).unwrapKey().orElseThrow().identifier().toString();
            if(needed.remove(biome)) {locations.add(p);System.err.println("seed="+seed+" "+biome+" chunk="+Arrays.toString(p));}
        }
        if(!needed.isEmpty())throw new AssertionError("missing test biomes "+needed);
        return locations;
    }
    static void chunks(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var fill=NoiseBasedChunkGenerator.class.getDeclaredMethod("doFill",NoiseChunk.class,net.minecraft.world.level.chunk.ChunkAccess.class);fill.setAccessible(true);
        var picker=NoiseBasedChunkGenerator.class.getDeclaredMethod("createFluidPicker",NoiseGeneratorSettings.class);picker.setAccessible(true);
        var out=new JsonArray();
        for(long seed:new long[]{0,8675309}) for(String dimension:List.of("overworld","nether","end")) for(int[] p:locations(registries,seed,dimension)) {
            var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace(dimension));
            var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings);var source=source(registries,dimension);
            var generator=new NoiseBasedChunkGenerator(source,Holder.direct(settings));
            var heights=net.minecraft.world.level.LevelHeightAccessor.create(settings.noiseSettings().minY(),settings.noiseSettings().height());
            var chunk=new net.minecraft.world.level.chunk.ProtoChunk(new net.minecraft.world.level.ChunkPos(p[0],p[1]),net.minecraft.world.level.chunk.UpgradeData.EMPTY,heights,net.minecraft.world.level.chunk.PalettedContainerFactory.create(registries),null);
            int minY=heights.getMinY(),height=heights.getHeight(),sy=height/4;
            var grid=biomeGrid(source,state,SamplerContext.builder().enableCaches().build(),p[0],p[1],minY,height);
            chunk.fillBiomesFromNoise((qx,qy,qz)->grid.get(((qz&3)*4+(qx&3))*sy+(qy-minY/4)));
            var volume=new DensityVolume(16,height,16,p[0]*16,minY,p[1]*16);
            try(var noise=new NoiseChunk(state,null,settings,(Aquifer.FluidPicker)picker.invoke(null,settings),net.minecraft.world.level.levelgen.blending.Blender.empty(),volume)) {
                fill.invoke(generator,noise,chunk);
                var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("dimension",dimension);item.add("chunk",array(p[0],p[1]));item.add("noise",blocks(chunk));item.addProperty("biomes",biomeHash(grid));
                Map<Long,List<Holder<Biome>>> cache=new HashMap<>();cache.put(net.minecraft.world.level.ChunkPos.pack(p[0],p[1]),grid);var context=SamplerContext.builder().enableCaches().build();
                BiomeResolver lookup=(qx,qy,qz)-> {
                    int cx=qx>>2,cz=qz>>2;var values=cache.computeIfAbsent(net.minecraft.world.level.ChunkPos.pack(cx,cz),key->biomeGrid(source,state,context,cx,cz,minY,height));
                    return values.get(((qz&3)*4+(qx&3))*sy+Math.clamp(qy-minY/4,0,sy-1));
                };
                var biomeManager=new BiomeManager(lookup,BiomeManager.obfuscateSeed(seed));
                state.surfaceSystem().buildSurface(state,biomeManager,new WorldGenerationContext(generator,heights),chunk,noise,settings.materialRule().value(),source.possibleBiomes());
                item.add("surface",blocks(chunk));out.add(item);
            }
        }
        write(directory.resolve("chunks.json.gz"),out);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var directory=Path.of(args[0]);Files.createDirectories(directory);
        var registries=load();presets(directory);densities(registries,directory);biomes(registries,directory);chunks(registries,directory);
    }
}
