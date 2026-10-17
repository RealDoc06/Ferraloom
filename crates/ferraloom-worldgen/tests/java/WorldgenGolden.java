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
        for(var entry:MultiNoiseBiomeSourceParameterList.knownPresets().entrySet()) {
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
            for(var entry:Map.of("temperature",router.temperature(),"vegetation",router.vegetation(),"continents",router.continents(),"erosion",router.erosion(),"depth",router.depth(),"ridges",router.ridges(),"chunk_surface_level",router.chunkSurfaceLevel(),"final_density",router.finalDensity()).entrySet()) {
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
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var directory=Path.of(args[0]);Files.createDirectories(directory);
        var registries=load();presets(directory);densities(registries,directory);
    }
}
