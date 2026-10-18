import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;

/** Captures the native saved-data codec, activation and primitive-key traversal order. */
public final class TicketStorageVector {
    static String encode(TicketStorage storage) {return TicketStorage.CODEC.encodeStart(NbtOps.INSTANCE,storage).getOrThrow().toString();}
    static JsonArray forced(TicketStorage storage) {var out=new JsonArray();storage.getForceLoadedChunks().forEach((long key)->out.add(key));return out;}
    static void snapshot(JsonArray out,String operation,TicketStorage storage) {var row=new JsonObject();row.addProperty("operation",operation);row.addProperty("encoded",encode(storage));row.add("forced",forced(storage));out.add(row);}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var root=new JsonObject();var parsing=new JsonArray();
        for(String input:List.of(
            "{}","{tickets:[]}","{tickets:1}",
            "{tickets:[{chunk_pos:[I;0,0],type:forced,level:31}]}",
            "{tickets:[{chunk_pos:[I;-3,7],type:portal,level:30,ticks_left:123L}]}",
            "{tickets:[{chunk_pos:[I;1,2],type:unknown,level:31,ticks_left:-1L}]}",
            "{tickets:[{chunk_pos:[I;1,2],type:portal,level:31,ticks_left:-1L}]}",
            "{tickets:[{chunk_pos:[I;0,0],type:forced,level:31},{chunk_pos:[I;0,0],type:forced,level:31}]}",
            "{tickets:[{chunk_pos:[I;0,0],type:portal,level:31,ticks_left:7L},{chunk_pos:[I;0,0],type:portal,level:31,ticks_left:3L}]}",
            "{tickets:[{type:forced,level:31}]}","{tickets:[{chunk_pos:[I;0],type:forced,level:31}]}",
            "{tickets:[{chunk_pos:[I;0,0],type:missing,level:31}]}","{tickets:[{chunk_pos:[I;0,0],type:forced}]}",
            "{tickets:[{chunk_pos:[I;0,0],type:forced,level:-1}]}","{tickets:[{chunk_pos:[I;0,0],type:forced,level:31,ticks_left:bad}]}")) {
            var row=new JsonObject();row.addProperty("input",input);var tag=TagParser.create(NbtOps.INSTANCE).parseFully(input);var result=TicketStorage.CODEC.parse(NbtOps.INSTANCE,tag);row.addProperty("success",result.isSuccess());result.error().ifPresent(error->row.addProperty("error",error.message()));
            result.resultOrPartial(error->{}).ifPresent(storage->{row.addProperty("before",encode(storage));row.add("forced_before",forced(storage));storage.activateAllDeactivatedTickets();row.addProperty("after",encode(storage));row.add("forced_after",forced(storage));});parsing.add(row);
        }
        root.add("parsing",parsing);var mutations=new JsonArray();var storage=new TicketStorage();
        for(int i=0;i<24;i++)storage.updateChunkForced(new ChunkPos(i%6-3,i/6-2),true);
        snapshot(mutations,"add-24",storage);storage.updateChunkForced(new ChunkPos(-3,-2),true);snapshot(mutations,"duplicate",storage);
        storage.updateChunkForced(new ChunkPos(0,0),false);snapshot(mutations,"remove-zero",storage);storage.updateChunkForced(new ChunkPos(-3,-2),false);snapshot(mutations,"remove-negative",storage);
        storage.addTicket(new Ticket(TicketType.PORTAL,30),new ChunkPos(6,7));storage.addTicket(new Ticket(TicketType.PLAYER_LOADING,31),new ChunkPos(8,9));snapshot(mutations,"persistent-types",storage);
        root.add("mutations",mutations);Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(root)+"\n");
    }
}
