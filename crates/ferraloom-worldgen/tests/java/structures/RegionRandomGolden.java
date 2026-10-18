import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.world.level.levelgen.*;

/** WorldGenRegion's independent positional stream for both random algorithms. */
@SuppressWarnings("deprecation")
public final class RegionRandomGolden {
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var input=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD).value();
        var out=new JsonArray();
        for(boolean legacy:new boolean[]{false,true})for(long seed:new long[]{0,-1,8675309,Long.MIN_VALUE,Long.MAX_VALUE}) {
            var settings=new NoiseGeneratorSettings(input.noiseSettings(),input.defaultBlock(),input.defaultFluid(),input.noiseRouter(),input.materialRule(),input.spawnTarget(),input.seaLevel(),input.disableMobGeneration(),input.aquifers(),legacy,input.debugFunctions());
            var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings);
            for(int[] p:new int[][]{{0,0},{-7,11},{1874999,-1875000}}) {
                var random=state.getOrCreateRandomFactory(Identifier.withDefaultNamespace("worldgen_region_random")).at(new BlockPos(p[0]*16,0,p[1]*16));
                var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("legacy",legacy);item.add("chunk",StructureGolden.ints(p));var values=new JsonArray();
                for(int i=0;i<12;i++) {var sample=new JsonObject();sample.addProperty("variant",random.nextBoolean());var roof=new JsonArray();for(int j=0;j<25;j++)roof.add(Integer.toUnsignedLong(Float.floatToRawIntBits(random.nextFloat())));sample.add("roof",roof);values.add(sample);}
                item.add("values",values);item.addProperty("next",random.nextInt());out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("region_random.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
