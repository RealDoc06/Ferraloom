import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.levelgen.*;
public final class BaseColumnGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        for(long seed:new long[]{0,-1,8675309})for(String dimension:List.of("overworld","nether","end")) {
            var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(net.minecraft.resources.Identifier.withDefaultNamespace(dimension));
            var random=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings);
            var generator=new NoiseBasedChunkGenerator(WorldgenGolden.source(registries,dimension),Holder.direct(settings));
            for(int[] p:new int[][]{{0,0},{-112,-208},{15,15},{2248,680},{792,-1112},{-2800,3248}})for(int[] bounds:new int[][]{{-64,384},{0,256},{32,16},{512,16}}) {
                var accessor=LevelHeightAccessor.create(bounds[0],bounds[1]);
                var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("dimension",dimension);item.add("position",FeaturesGolden.values(p));item.add("bounds",FeaturesGolden.values(bounds));
                var heights=new JsonArray();for(var map:List.of(Heightmap.Types.WORLD_SURFACE,Heightmap.Types.OCEAN_FLOOR,Heightmap.Types.MOTION_BLOCKING,Heightmap.Types.MOTION_BLOCKING_NO_LEAVES)) {int y=generator.getBaseHeight(p[0],p[1],map,accessor,random);if(y!=generator.getFirstFreeHeight(p[0],p[1],map,accessor,random)||y-1!=generator.getFirstOccupiedHeight(p[0],p[1],map,accessor,random))throw new AssertionError();heights.add(y);}item.add("heights",heights);
                var column=generator.getBaseColumn(p[0],p[1],accessor,random);var blocks=new JsonArray();if(column!=null)for(int y=bounds[0]-1;y<=bounds[0]+bounds[1];y++)blocks.add(Block.getId(column.getBlock(y)));item.add("column",column==null?JsonNull.INSTANCE:blocks);cases.add(item);
            }
        }
        WorldgenGolden.write(out.resolve("base_columns.json.gz"),cases);
    }
}
