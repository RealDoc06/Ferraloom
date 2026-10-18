import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;

/** Native StructureStart load rebuilds monument children from seed and saved origin. */
public final class MonumentReloadGolden {
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        for(long seed:new long[]{0,18,-1})for(int[] chunk:new int[][]{{0,0},{-7,11}})for(int orientation=-1;orientation<4;orientation++)for(boolean shifted:new boolean[]{false,true}) {
            String box=shifted?"-91,-11,73,-63,20,90":"-46,39,8,11,61,65";
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{id:'minecraft:monument',ChunkX:"+chunk[0]+",ChunkZ:"+chunk[1]+",references:7,Children:[{id:'minecraft:omb',O:"+orientation+",GD:0,BB:[I;"+box+"]}]}");
            var start=StructureStart.loadStaticStart(serialization,tag,seed);var children=new JsonArray();
            for(Object child:(List<?>)MonumentGolden.field(start.getPieces().getFirst(),"childPieces")) {
                var value=new JsonObject();value.addProperty("nbt",((StructurePiece)child).createTag(serialization).toString());
                try {value.addProperty("design",(Integer)MonumentGolden.field(child,"mainDesign"));}catch(NoSuchFieldException ignored){}
                children.add(value);
            }
            var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("initial",tag.toString());item.addProperty("loaded",start.createTag(serialization,new ChunkPos(chunk[0],chunk[1])).toString());item.add("children",children);out.add(item);
        }
        StructureGolden.write(directory.resolve("monument_reload.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
