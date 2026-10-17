import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
public final class HeightmapGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        for(String definition:List.of("[{op:'query'},{op:'bulk',y:10,block:'stone'},{op:'query'},{op:'set',y:2,block:'stone'},{op:'query'},{op:'set',y:2,block:'air'},{op:'query'},{op:'set',y:15,block:'water'},{op:'query'},{op:'set',y:15,block:'air'},{op:'query'},{op:'set',y:0,block:'air'},{op:'query'}]","[{op:'bulk',y:10,block:'stone'},{op:'query'},{op:'set',y:10,block:'air'},{op:'query'}]")) {
            var chunk=new ProtoChunk(new ChunkPos(0,0),UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),null);
            chunk.setBlockState(new BlockPos(0,0,0),Blocks.STONE.defaultBlockState());
            chunk.setPersistedStatus(ChunkStatus.TERRAIN);
            var input=JsonParser.parseString(definition);var output=new JsonArray();
            for(var op:input.getAsJsonArray()) {
                var object=op.getAsJsonObject();String kind=object.get("op").getAsString();
                if(kind.equals("query")) {var values=new JsonArray();for(var map:Heightmap.Types.values())values.add(chunk.getHeight(map,0,0)+1);output.add(values);}
                else {int y=object.get("y").getAsInt();var state=BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(object.get("block").getAsString())).defaultBlockState();if(kind.equals("bulk"))chunk.getSection(chunk.getSectionIndex(y)).setBlockState(0,y&15,0,state);else chunk.setBlockState(new BlockPos(0,y,0),state);}
            }
            var item=new JsonObject();item.add("ops",input);item.add("output",output);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("feature_heightmaps.json.gz"),cases);
    }
}
