import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.structures.*;

/** Real monument graph connectivity, source scans, ordered fitters and child bounds. */
public final class MonumentGolden {
    static Object field(Object object,String name)throws Exception {
        for(Class<?> type=object.getClass();type!=null;type=type.getSuperclass()) {
            try {var f=type.getDeclaredField(name);f.setAccessible(true);return f.get(object);}catch(NoSuchFieldException ignored){}
        }
        throw new NoSuchFieldException(name);
    }
    static void collect(Object room,TreeMap<Integer,Object> rooms)throws Exception {
        int index=(Integer)field(room,"index");if(rooms.putIfAbsent(index,room)!=null)return;
        for(Object neighbor:(Object[])field(room,"connections"))if(neighbor!=null)collect(neighbor,rooms);
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        for(long sample=0;sample<32;sample++)for(var direction:Direction.Plane.HORIZONTAL) {
            long seed=sample*341873128712L+132897987541L;var random=new LegacyRandomSource(seed);
            var piece=new OceanMonumentPieces.MonumentBuilding(random,-46,8,direction);var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("direction",direction.get2DDataValue());item.addProperty("nbt",piece.createTag(serialization).toString());item.addProperty("next",random.nextInt());
            var rooms=new TreeMap<Integer,Object>();collect(field(piece,"sourceRoom"),rooms);item.addProperty("core",(Integer)field(field(piece,"coreRoom"),"index"));
            var graph=new JsonArray();
            for(var entry:rooms.entrySet()) {
                var room=entry.getValue();var value=new JsonObject();value.addProperty("index",entry.getKey());value.addProperty("claimed",(Boolean)field(room,"claimed"));value.addProperty("source",(Boolean)field(room,"isSource"));value.addProperty("scan",(Integer)field(room,"scanIndex"));
                var connections=new JsonArray();for(Object neighbor:(Object[])field(room,"connections"))connections.add(neighbor==null?-1:(Integer)field(neighbor,"index"));value.add("connections",connections);
                var openings=new JsonArray();for(boolean opening:(boolean[])field(room,"hasOpening"))openings.add(opening);value.add("openings",openings);graph.add(value);
            }
            item.add("rooms",graph);var children=new JsonArray();
            for(Object child:(List<?>)field(piece,"childPieces")) {
                var value=new JsonObject();value.addProperty("nbt",((StructurePiece)child).createTag(serialization).toString());
                Object room=field(child,"roomDefinition");value.addProperty("room",room==null?-1:(Integer)field(room,"index"));
                try {value.addProperty("design",(Integer)field(child,"mainDesign"));}catch(NoSuchFieldException ignored){}
                children.add(value);
            }
            item.add("children",children);out.add(item);
        }
        StructureGolden.write(directory.resolve("monument_pieces.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
