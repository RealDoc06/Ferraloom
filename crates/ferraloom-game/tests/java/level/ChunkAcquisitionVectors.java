import java.util.ArrayList;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;

/** Executes native ChunkLevel and UNKNOWN ticket expiration, with no world IO. */
public final class ChunkAcquisitionVectors {
    public static void main(String[] args) {
        var out = System.out;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(out);
        for (var status : ChunkStatus.getStatusList())
            out.println("level\t"+status.getName()+"\t"+ChunkLevel.byStatus(status));
        var storage = new TicketStorage();
        var pos = new ChunkPos(0,0);
        var trace = new ArrayList<String>();
        storage.addTicket(new Ticket(TicketType.UNKNOWN,33),pos);
        trace.add(""+storage.getTickets(pos.pack()).size());
        storage.purgeStaleTickets(null); // UNKNOWN can expire without a holder.
        trace.add(""+storage.getTickets(pos.pack()).size());
        storage.addTicket(new Ticket(TicketType.UNKNOWN,33),pos);
        trace.add(""+storage.getTickets(pos.pack()).size());
        for(int i=0;i<2;i++) {
            storage.purgeStaleTickets(null);
            trace.add(""+storage.getTickets(pos.pack()).size());
        }
        out.println("unknown-refresh\t"+String.join(",",trace));
    }
}
