import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Authored current-version start codec edge cases, independent of generation. */
public final class StructureStartCodecGolden {
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("start-codec-world")).createAccess("test")) {
            var templates=new StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var context=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            var cases=new ArrayList<CompoundTag>();
            for(String box:List.of("[I;1,2,3,4,5,6]","[1,2,3,4,5,6]","[B;1b,2b,3b,4b,5b,6b]","[L;4294967297L,2L,3L,4L,5L,6L]","[-1.9f,-2.9f,-3.9f,4.9f,5.9f,6.9f]","[-1.9d,-2.9d,-3.9d,4.9d,5.9d,6.9d]","['bad',2,3,4,5,6]","[I;1,2,3,4,5]","[I;6,5,4,3,2,1]")) {
                for(String id:List.of("minecraft:btp","MINECRAFT:BTP","minecraft:nvi","minecraft:pcp","minecraft:bastionremnant","minecraft:runtime","minecraft:jigsaw","minecraft:omb","minecraft:omentry","minecraft:omcr","minecraft:omdxr","minecraft:omdyr","minecraft:omdzr","minecraft:omdxyr","minecraft:omdyzr","minecraft:omsimple","minecraft:omsimplet","minecraft:omwr","minecraft:ompenthouse")) {
                    String piece="{id:'"+id+"',BB:"+box+",O:-1,GD:2.9d,PosX:1,PosY:64,PosZ:3,rotation:'NONE',pool_element:{element_type:'minecraft:single_pool_element',location:'minecraft:village/plains/houses/plains_small_house_1',processors:'minecraft:empty',projection:'rigid'},junctions:[]}";
                    cases.add(TagParser.parseCompoundFully("{id:'minecraft:buried_treasure',ChunkX:-7,ChunkZ:11,references:3,Children:["+piece+"]}"));
                }
            }
            for(String child:List.of("{}","{BB:[I;0,1,2,3,4,5]}","{id:''}","{id:'minecraft:'}","{id:'minecraft:unknown_piece'}","{id:'BAD!'}","17","{id:'minecraft:btp'}"))cases.add(TagParser.parseCompoundFully("{id:'minecraft:buried_treasure',Children:[{id:'minecraft:btp',BB:[I;0,1,2,3,4,5]},"+child+",{id:'minecraft:btp',BB:[I;6,7,8,9,10,11]}]}"));
            for(String id:List.of("INVALID","","minecraft:","minecraft:unknown_structure","BAD!","MINECRAFT:buried_treasure"))cases.add(TagParser.parseCompoundFully("{id:'"+id+"',Children:[]}"));
            for(var tag:cases) {
                var item=new JsonObject();item.addProperty("input",tag.toString());
                try {var start=StructureStart.loadStaticStart(context,tag,0);if(start==null)item.addProperty("outcome","null");else {item.addProperty("outcome","start");item.addProperty("output",start.createTag(context,new ChunkPos(-7,11)).toString());}}
                catch(RuntimeException e) {item.addProperty("outcome","throws");}
                out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("structure_start_codec.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
