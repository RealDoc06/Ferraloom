import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;

/** Exact map traversal after real TicketStorage and SimulationChunkTracker updates. */
public final class SimulationVectors {
    static final class Tracker extends SimulationChunkTracker {
        Tracker(TicketStorage storage){super(storage);}
        void print(String stage){
            var rows=new ArrayList<String>();
            for(var entry:chunks.long2ByteEntrySet()){
                var pos=ChunkPos.unpack(entry.getLongKey());rows.add(pos.x()+","+pos.z()+","+entry.getByteValue());
            }
            System.out.println(stage+"\t"+String.join(";",rows));
        }
    }
    public static void main(String[] args){
        var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
        var storage=new TicketStorage();var tracker=new Tracker(storage);
        var a=new ChunkPos(0,0);var b=new ChunkPos(2,-1);var c=new ChunkPos(-2,1);
        storage.addTicket(new Ticket(TicketType.PLAYER_SIMULATION,30),a);tracker.runAllUpdates();tracker.print("origin");
        storage.addTicket(new Ticket(TicketType.PLAYER_SIMULATION,30),b);tracker.runAllUpdates();tracker.print("overlap");
        storage.removeTicket(new Ticket(TicketType.PLAYER_SIMULATION,30),a);tracker.runAllUpdates();tracker.print("remove-origin");
        storage.removeTicket(new Ticket(TicketType.PLAYER_SIMULATION,30),b);storage.addTicket(new Ticket(TicketType.PLAYER_SIMULATION,29),c);tracker.runAllUpdates();tracker.print("move");
        storage.removeTicket(new Ticket(TicketType.PLAYER_SIMULATION,29),c);tracker.runAllUpdates();tracker.print("empty");
    }
}
