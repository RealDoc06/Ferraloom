import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.util.RandomSource;
import net.minecraft.util.valueproviders.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.carver.*;
import net.minecraft.world.level.levelgen.heightproviders.*;

/** Our reference driver calls real 26.3 provider codecs and carver geometry. */
public final class FeaturesGolden {
    static final long[] SEEDS={0,1,-1,8675309,Long.MIN_VALUE,Long.MAX_VALUE};
    static JsonArray values(int... values) { var out=new JsonArray();for(int v:values)out.add(v);return out; }
    static String maskHash(CarvingMask mask) throws Exception {
        var digest=java.security.MessageDigest.getInstance("SHA-256");
        mask.visit((x,z,bottom,top)-> { for(int v:new int[]{x,z,bottom,top})for(int shift=24;shift>=0;shift-=8)digest.update((byte)(v>>shift)); });
        return HexFormat.of().formatHex(digest.digest());
    }
    static WorldGenerationContext context(RegistryAccess.Frozen registries,String dimension) {
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(net.minecraft.resources.Identifier.withDefaultNamespace(dimension));
        return new WorldGenerationContext(new NoiseBasedChunkGenerator(WorldgenGolden.source(registries,dimension),Holder.direct(settings)),LevelHeightAccessor.create(settings.noiseSettings().minY(),settings.noiseSettings().height()));
    }
    static void masks(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        var registry=registries.lookupOrThrow(Registries.CARVER);
        for(var entry:registry.listElements().sorted(Comparator.comparing(e->e.key().identifier())).toList()) {
            String name=entry.key().identifier().toString();var carver=entry.value();
            var context=context(registries,name.contains("nether")?"nether":"overworld");
            for(long seed:SEEDS)for(int[] p:new int[][]{{0,0},{-7,11}}) {
                var mask=new CarvingMask(context.getMinGenY()+1,context.getMinGenY()+context.getGenDepth()-8);
                var random=new WorldgenRandom(new LegacyRandomSource(0));
                for(int dx=-8;dx<=8;dx++)for(int dz=-8;dz<=8;dz++) {
                    var source=new ChunkPos(p[0]+dx,p[1]+dz);
                    random.setLargeFeatureSeed(seed,source.x(),source.z());
                    if(carver.isStartChunk(random))carver.carve(context,random,new ChunkPos(p[0],p[1]),source,mask);
                }
                var item=new JsonObject();item.addProperty("carver",name);item.addProperty("seed",seed);item.add("chunk",values(p));item.addProperty("hash",maskHash(mask));item.addProperty("next",random.nextLong());cases.add(item);
            }
        }
        WorldgenGolden.write(out.resolve("carvers.json.gz"),cases);
    }
    static void providers(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();var context=context(registries,"overworld");
        var definitions=List.of(
            new String[]{"int","7"},
            new String[]{"int","{type:'minecraft:constant',value:-3}"},
            new String[]{"int","{type:'minecraft:uniform',min_inclusive:-9,max_inclusive:20}"},
            new String[]{"int","{type:'minecraft:biased_to_bottom',min_inclusive:-9,max_inclusive:20}"},
            new String[]{"int","{type:'minecraft:very_biased_to_bottom',min_inclusive:-9,max_inclusive:20}"},
            new String[]{"int","{type:'minecraft:clamped',source:{type:'minecraft:uniform',min_inclusive:-10,max_inclusive:20},min_inclusive:2,max_inclusive:7}"},
            new String[]{"int","{type:'minecraft:clamped_normal',mean:2.25,deviation:4.5,min_inclusive:-9,max_inclusive:20}"},
            new String[]{"int","{type:'minecraft:trapezoid',min:-7,max:7,plateau:0}"},
            new String[]{"int","{type:'minecraft:trapezoid',min:-9,max:20,plateau:7}"},
            new String[]{"int","{type:'minecraft:trapezoid',min:-9,max:20,plateau:29}"},
            new String[]{"int","{type:'minecraft:weighted_list',distribution:[{data:4,weight:0},{data:{type:'minecraft:uniform',min_inclusive:-10,max_inclusive:20},weight:3},{data:17,weight:4}]}"},
            new String[]{"float","1.25"},
            new String[]{"float","{type:'minecraft:constant',value:0.5}"},
            new String[]{"float","{type:'minecraft:uniform',min_inclusive:-2.25,max_exclusive:5.75}"},
            new String[]{"float","{type:'minecraft:clamped_normal',mean:1.25,deviation:4.5,min:-2.25,max:5.75}"},
            new String[]{"float","{type:'minecraft:trapezoid',min:-2.25,max:5.75,plateau:3.5}"},
            new String[]{"height","{relative_to_sea_level:5}"},
            new String[]{"height","{type:'minecraft:constant',value:{below_top:2}}"},
            new String[]{"height","{type:'minecraft:uniform',min_inclusive:{above_bottom:5},max_inclusive:{below_top:2}}"},
            new String[]{"height","{type:'minecraft:uniform',min_inclusive:{absolute:30},max_inclusive:{absolute:20}}"},
            new String[]{"height","{type:'minecraft:biased_to_bottom',min_inclusive:{above_bottom:5},max_inclusive:{below_top:2},inner:7}"},
            new String[]{"height","{type:'minecraft:very_biased_to_bottom',min_inclusive:{above_bottom:5},max_inclusive:{below_top:2},inner:7}"},
            new String[]{"height","{type:'minecraft:very_biased_to_bottom',min_inclusive:{absolute:3},max_inclusive:{absolute:3}}"},
            new String[]{"height","{type:'minecraft:trapezoid',min_inclusive:{above_bottom:5},max_inclusive:{below_top:2},plateau:17}"},
            new String[]{"height","{type:'minecraft:weighted_list',distribution:[{data:{absolute:20},weight:1},{data:{above_bottom:20},weight:3}]}"}
        );
        for(var definition:definitions) {
            var config=JsonParser.parseString(definition[1]);
            for(long seed:SEEDS) {
                var item=new JsonObject();item.addProperty("type",definition[0]);item.add("config",config);item.addProperty("seed",seed);var samples=new JsonArray();var random=new WorldgenRandom(new XoroshiroRandomSource(seed));
                switch(definition[0]) {
                    case "int" -> { var p=IntProviders.CODEC.parse(JsonOps.INSTANCE,config).getOrThrow();for(int i=0;i<64;i++)samples.add(p.sample(random)); }
                    case "float" -> { var p=FloatProviders.CODEC.parse(JsonOps.INSTANCE,config).getOrThrow();for(int i=0;i<64;i++)samples.add(Integer.toUnsignedLong(Float.floatToRawIntBits(p.sample(random)))); }
                    case "height" -> { var p=HeightProvider.CODEC.parse(JsonOps.INSTANCE,config).getOrThrow();for(int i=0;i<64;i++)samples.add(p.sample(random,context)); }
                }
                item.add("samples",samples);item.addProperty("next",random.nextLong());cases.add(item);
            }
        }
        WorldgenGolden.write(out.resolve("providers.json.gz"),cases);
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
        var registries=WorldgenGolden.load();var out=Path.of(args[0]);Files.createDirectories(out);
        providers(registries,out);masks(registries,out);
    }
}
