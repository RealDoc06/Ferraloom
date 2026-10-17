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
    static void blending(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var constructor=net.minecraft.world.level.levelgen.blending.BlendingData.class.getDeclaredConstructor(int.class,int.class,Optional.class);constructor.setAccessible(true);
        var densities=net.minecraft.world.level.levelgen.blending.BlendingData.class.getDeclaredField("densities");densities.setAccessible(true);
        var biomes=net.minecraft.world.level.levelgen.blending.BlendingData.class.getDeclaredField("biomes");biomes.setAccessible(true);
        var heightsMap=new it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap<net.minecraft.world.level.levelgen.blending.BlendingData>();
        var densityMap=new it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap<net.minecraft.world.level.levelgen.blending.BlendingData>();
        var biomeRegistry=registries.lookupOrThrow(Registries.BIOME);
        for(int[] p:new int[][]{{-1,-1},{-1,0},{0,-1},{0,0},{2,1}}) {
            int cx=p[0],cz=p[1],min=cx<0?-4:0;float[] h=new float[16];
            for(int i=0;i<16;i++)h[i]=((i+cx+cz)&3)==0?Float.MAX_VALUE:20+cx*9+cz*3+i*1.5f;
            var data=constructor.newInstance(min,min+4,Optional.of(h));
            float[][] ds=(float[][])densities.get(data);
            @SuppressWarnings("unchecked") var bs=(List<List<Holder<Biome>>>)biomes.get(data);
            for(int i=0;i<16;i++) {
                if(i%3!=0) {ds[i]=new float[8];for(int y=0;y<8;y++)ds[i][y]=((i*17+y*5+cx-cz)%29-14)/4f;}
                if(i%2==0) {var column=new ArrayList<Holder<Biome>>();for(int y=0;y<16;y++)column.add(biomeRegistry.getOrThrow(((i+y+cx)&1)==0?Biomes.PLAINS:Biomes.DESERT));bs.set(i,column);}
            }
            heightsMap.put(net.minecraft.world.level.ChunkPos.pack(cx,cz),data);if(Math.abs(cx)<=1&&Math.abs(cz)<=1)densityMap.put(net.minecraft.world.level.ChunkPos.pack(cx,cz),data);
        }
        var ctor=net.minecraft.world.level.levelgen.blending.Blender.class.getDeclaredConstructors()[0];ctor.setAccessible(true);
        var blender=(net.minecraft.world.level.levelgen.blending.Blender)ctor.newInstance(heightsMap,densityMap,null);
        var offset=net.minecraft.world.level.levelgen.blending.Blender.class.getDeclaredMethod("blendOffsetAndFactor",int.class,int.class);offset.setAccessible(true);
        var biome=net.minecraft.world.level.levelgen.blending.Blender.class.getDeclaredMethod("blendBiome",int.class,int.class,int.class);biome.setAccessible(true);
        var result=new JsonArray();
        var cached=blender.blendOffsetAndFactor(new DensityVolume(16,64,16,-16,-64,-16));
        for(int x:new int[]{-129,-33,-17,-16,-4,-1,0,1,4,15,16,17,63,129})for(int z:new int[]{-17,-4,0,4,17})for(int y:new int[]{-65,-32,-1,0,7,8,31,63,64}) {
            var output=offset.invoke(blender,x,z);var alpha=output.getClass().getDeclaredMethod("alpha");alpha.setAccessible(true);var value=output.getClass().getDeclaredMethod("blendingOffset");value.setAccessible(true);
            var row=new JsonArray();row.add(array(x,y,z));row.add(Integer.toUnsignedLong(Float.floatToRawIntBits((float)alpha.invoke(output))));row.add(Integer.toUnsignedLong(Float.floatToRawIntBits((float)value.invoke(output))));row.add(Integer.toUnsignedLong(Float.floatToRawIntBits(blender.blendDensity(x,y,z,0.12345f))));
            var b=(Holder<?>)biome.invoke(blender,x>>2,y>>2,z>>2);row.add(b==null?JsonNull.INSTANCE:new JsonPrimitive(b.unwrapKey().orElseThrow().identifier().toString()));
            row.add(Integer.toUnsignedLong(Float.floatToRawIntBits(cached.getAlpha(x,z))));row.add(Integer.toUnsignedLong(Float.floatToRawIntBits(cached.getOffset(x,z))));result.add(row);
        }
        write(directory.resolve("blending.json.gz"),result);
    }
    static void codecsAndOperations(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var ops=RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE,registries);
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),0,settings);
        var result=new JsonArray();
        String[][] cases={
            {"density","0.0"},
            {"density","-0.0"},
            {"density","1000000.0"},
            {"density","true"},
            {"density","{}"},
            {"density","\"minecraft:zero\""},
            {"density","{\"type\":\"minecraft:blend_alpha\"}"},
            {"density","{\"type\":\"minecraft:blend_offset\"}"},
            {"density","{\"type\":\"minecraft:beardifier\"}"},
            {"density","{\"type\":\"minecraft:end_outer_islands\"}"},
            {"density","{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}"},
            {"density","{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25,\"tiling\":\"repeat\"}"},
            {"density","{\"type\":\"minecraft:gradient\",\"axis\":\"z\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25,\"tiling\":\"mirrored_repeat\"}"},
            {"density","{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":17,\"to_coordinate\":-31,\"from_value\":-3.875,\"to_value\":5.25}"},
            {"density","{\"type\":\"minecraft:shift\",\"noise\":\"minecraft:surface\"}"},
            {"density","{\"type\":\"minecraft:shift_a\",\"noise\":\"minecraft:surface\"}"},
            {"density","{\"type\":\"minecraft:shift_b\",\"noise\":\"minecraft:surface\"}"},
            {"density","{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5}"},
            {"density","{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5,\"shift_x\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"shift_y\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"shift_z\":0.75}"},
            {"density","{\"type\":\"minecraft:distance_to_point\",\"point\":[17,-7,12],\"metric\":\"euclidean\"}"},
            {"density","{\"type\":\"minecraft:distance_to_point\",\"point\":[17,-7,12],\"metric\":\"euclidean_squared\"}"},
            {"density","{\"type\":\"minecraft:distance_to_point\",\"point\":[17,-7,12],\"metric\":\"manhattan\"}"},
            {"density","{\"type\":\"minecraft:distance_to_point\",\"point\":[17,-7,12],\"metric\":\"chebyshev\"}"},
            {"density","{\"type\":\"minecraft:abs\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:square\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:cube\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:sqrt\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:half_negative\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:quarter_negative\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:reciprocal\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:negate\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:squeeze\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:log\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:sign\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:add\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:add\",\"left\":2.25,\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:add\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":0.3}"},
            {"density","{\"type\":\"minecraft:sub\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:sub\",\"left\":2.25,\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:sub\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":0.3}"},
            {"density","{\"type\":\"minecraft:mul\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:mul\",\"left\":2.25,\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:mul\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":0.3}"},
            {"density","{\"type\":\"minecraft:div\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:div\",\"left\":2.25,\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:div\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":0.3}"},
            {"density","{\"type\":\"minecraft:min\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:min\",\"left\":2.25,\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:min\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":0.3}"},
            {"density","{\"type\":\"minecraft:max\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:max\",\"left\":2.25,\"right\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:max\",\"left\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"right\":0.3}"},
            {"density","{\"type\":\"minecraft:floor\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:floor\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0.75}"},
            {"density","{\"type\":\"minecraft:floor\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0}"},
            {"density","{\"type\":\"minecraft:round\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:round\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0.75}"},
            {"density","{\"type\":\"minecraft:round\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0}"},
            {"density","{\"type\":\"minecraft:ceil\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:ceil\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0.75}"},
            {"density","{\"type\":\"minecraft:ceil\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0}"},
            {"density","{\"type\":\"minecraft:truncate\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:truncate\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0.75}"},
            {"density","{\"type\":\"minecraft:truncate\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"multiple\":0}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":-3}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":-2}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":-1}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":-0.5}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":0}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":0.5}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":1}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":2}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":3}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":1.3}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":3.75,\"exponent\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:pow\",\"base\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"exponent\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25}}"},
            {"density","{\"type\":\"minecraft:clamp\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"min\":-0.125,\"max\":0.875}"},
            {"density","{\"type\":\"minecraft:lerp\",\"alpha\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"first\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"second\":{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5}}"},
            {"density","{\"type\":\"minecraft:range_choice\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"min_inclusive\":-1,\"max_exclusive\":1,\"when_in_range\":{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"when_out_of_range\":{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5}}"},
            {"density","{\"type\":\"minecraft:interval_select\",\"input\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"thresholds\":[-1,-1,0,0.5],\"functions\":[{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},{\"type\":\"minecraft:gradient\",\"axis\":\"x\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5},0,1]}"},
            {"density","{\"type\":\"minecraft:interval_select\",\"input\":{\"type\":\"minecraft:log\",\"input\":-1},\"thresholds\":[-1,0,0.5],\"functions\":[0,1,2,3]}"},
            {"density","{\"type\":\"minecraft:cache\",\"input\":{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5}}"},
            {"density","{\"type\":\"minecraft:blend_density\",\"input\":{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5}}"},
            {"density","{\"type\":\"minecraft:interpolated\",\"input\":{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5},\"cell_size_xz\":3,\"cell_size_y\":7}"},
            {"density","{\"type\":\"minecraft:slice\",\"axis\":\"x\",\"coordinate\":13,\"input\":{\"type\":\"minecraft:noise\",\"noise\":\"minecraft:surface\",\"xz_scale\":0.25,\"y_scale\":0.5}}"},
            {"density","{\"type\":\"minecraft:find_top_surface\",\"density\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"upper_bound\":45,\"lower_bound\":-31,\"cell_height\":7}"},
            {"density","{\"type\":\"minecraft:spline\",\"spline\":{\"coordinate\":{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":-32,\"to_coordinate\":32,\"from_value\":-3.875,\"to_value\":5.25},\"points\":[{\"location\":-1.25,\"value\":-3.5,\"derivative\":0.75},{\"location\":1.25,\"value\":5.5,\"derivative\":-0.35}]}}"},
            {"density","{\"type\":\"minecraft:spline\",\"spline\":{\"coordinate\":{\"type\":\"minecraft:log\",\"input\":-1},\"points\":[{\"location\":-1,\"value\":1,\"derivative\":0},{\"location\":1,\"value\":2,\"derivative\":0}]}}"},
            {"density","{\"type\":\"minecraft:clamp\",\"input\":-0.0,\"min\":0,\"max\":1}"},
            {"density","{\"type\":\"minecraft:clamp\",\"input\":0,\"min\":1,\"max\":0}"},
            {"density","{\"type\":\"minecraft:gradient\",\"axis\":\"y\",\"from_coordinate\":0,\"to_coordinate\":0,\"from_value\":0,\"to_value\":1}"},
            {"density","{\"type\":\"minecraft:interpolated\",\"input\":1,\"cell_size_xz\":0,\"cell_size_y\":1}"},
            {"density","{\"type\":\"minecraft:spline\",\"spline\":{\"coordinate\":0,\"points\":[]}}"},
            {"density","{\"type\":\"minecraft:interval_select\",\"input\":0,\"thresholds\":[1,0],\"functions\":[0,1,2]}"},
            {"noise","{\"base_octave\":-5}"},
            {"noise","{\"base_octave\":0,\"normalize\":false}"},
            {"noise","{\"base_octave\":0,\"normalize\":\"legacy\"}"},
            {"noise","{\"base_octave\":0,\"normalize\":\"enabled\"}"},
            {"noise","{\"base_octave\":0,\"octave_count\":2,\"amplitude_modifiers\":[1]}"},
            {"noise","{\"base_octave\":0,\"octave_count\":2,\"amplitude_modifiers\":[]}"},
            {"noise","{\"base_octave\":33}"},
            {"noise","{\"base_octave\":0,\"base_amplitude\":0}"},
            {"noise","{\"base_octave\":-0.5}"},
            {"noise","{\"base_octave\":true}"},
            {"noise_settings","{\"min_y\":-64,\"height\":384}"},
            {"noise_settings","{\"min_y\":0,\"height\":0}"},
            {"noise_settings","{\"min_y\":-2032,\"height\":4064}"},
            {"noise_settings","{\"min_y\":-63,\"height\":384}"},
            {"noise_settings","{\"min_y\":0,\"height\":4096}"},
            {"noise_settings","{\"min_y\":2032,\"height\":0}"},
            {"noise_settings","{\"min_y\":0,\"height\":15.99}"},
            {"block_state","\"stone\""},
            {"block_state","{\"id\":\"oak_stairs\",\"properties\":{\"waterlogged\":\"true\",\"facing\":\"west\",\"half\":\"bogus\"}}"},
            {"block_state","{\"id\":\"grass_block\",\"properties\":{\"snowy\":true}}"},
            {"block_state","{\"id\":\"stone\",\"properties\":4}"},
            {"block_state","{\"id\":\"oak_stairs\",\"properties\":{\"waterlogged\":true}}"},
            {"block_state","{\"Name\":\"stone\"}"},
            {"parameter","0"},
            {"parameter","[-2,2]"},
            {"parameter","{\"min\":0,\"max\":0}"},
            {"parameter","[1,-1]"},
            {"parameter","[-2.00000001,2.00000001]"},
            {"parameter","[]"},
            {"parameter","[0,1,2]"},
            {"parameter","true"},
            {"anchor","{\"absolute\":0}"},
            {"anchor","{\"absolute\":0,\"above_bottom\":1}"},
            {"anchor","{\"absolute\":\"bad\",\"above_bottom\":1}"},
            {"anchor","{\"relative_to_sea_level\":-5}"},
            {"anchor","5"},
            {"condition","{\"type\":\"minecraft:biome\",\"biome_is\":[\"minecraft:plains\",\"minecraft:plains\"]}"},
            {"condition","{\"type\":\"minecraft:noise_threshold\",\"noise\":\"minecraft:surface\",\"min_threshold\":0,\"max_threshold\":1}"},
            {"condition","{\"type\":\"minecraft:water\",\"offset\":0,\"surface_depth_multiplier\":21,\"add_stone_depth\":false}"},
            {"condition","{\"type\":\"minecraft:stone_depth\",\"offset\":0,\"add_surface_depth\":true,\"secondary_depth_range\":-1,\"surface_type\":\"floor\"}"},
            {"material","{\"type\":\"minecraft:sequence\",\"sequence\":[]}"},
            {"material","{\"type\":\"minecraft:block\",\"result_state\":\"stone\"}"},
            {"material","{\"type\":\"minecraft:ore_vein\",\"ore_block\":\"iron_ore\",\"raw_ore_block\":\"raw_iron_block\",\"filler_block\":\"tuff\",\"raw_ore_chance\":1.00000001,\"density\":1,\"richness\":1,\"filler_gap\":-1}"},
        };
        for(var test:cases) {
            var definition=JsonParser.parseString(test[1]);
            com.mojang.serialization.Codec<?> codec=switch(test[0]) {
                case "density" -> DensityFunction.CODEC;
                case "noise" -> net.minecraft.world.level.levelgen.synth.NormalNoise.DIRECT_CODEC;
                case "noise_settings" -> NoiseSettings.CODEC;
                case "block_state" -> net.minecraft.world.level.block.state.BlockState.CODEC;
                case "parameter" -> Climate.Parameter.CODEC;
                case "anchor" -> VerticalAnchor.CODEC;
                case "condition" -> net.minecraft.world.level.levelgen.material.condition.MaterialCondition.CODEC;
                default -> net.minecraft.world.level.levelgen.material.rule.MaterialRule.CODEC;
            };
            var parsed=codec.parse(ops,definition).result();var row=new JsonObject();row.addProperty("codec",test[0]);row.add("input",definition);row.addProperty("accepted",parsed.isPresent());
            if(parsed.isPresent() && test[0].equals("block_state")) row.addProperty("state",net.minecraft.world.level.block.Block.getId((net.minecraft.world.level.block.state.BlockState)parsed.get()));
            if(parsed.isPresent() && test[0].equals("density")) {
                var sampler=state.getSampler((DensityFunction)parsed.get());var values=new JsonArray();
                for(int[] p:POINTS)values.add(Integer.toUnsignedLong(Float.floatToIntBits(sampler.sampleValue(SamplerContext.EMPTY_UNCACHED,p[0],p[1],p[2]))));row.add("values",values);
                var volume=new DensityVolume(3,5,2,-17,-63,31,4,3,7);var context=SamplerContext.builder().enableCaches().build();
                sampler.sampleValue(context,-17,-63,31);
                var data=new JsonArray();try(var buffer=context.acquireBuffer(volume)) {sampler.sampleVolume(context,buffer,volume);for(int i=0;i<volume.size();i++)data.add(Integer.toUnsignedLong(Float.floatToIntBits(buffer.get(i))));}row.add("volume",data);
                var cached=new JsonArray();for(int[] p:new int[][]{{-17,-63,31},{-13,-60,38},{17,-7,31},{-17,-63,31}})cached.add(Integer.toUnsignedLong(Float.floatToIntBits(sampler.sampleValue(context,p[0],p[1],p[2]))));row.add("cached",cached);
            }
            result.add(row);
        }
        write(directory.resolve("codecs_and_operations.json.gz"),result);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var directory=Path.of(args[0]);Files.createDirectories(directory);
        var registries=load();presets(directory);densities(registries,directory);biomes(registries,directory);chunks(registries,directory);blending(registries,directory);codecsAndOperations(registries,directory);
    }
}
