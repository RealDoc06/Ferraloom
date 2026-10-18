import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;

/** Real LoadingChunkTracker with real holders; no generation task is submitted. */
public final class LoadingVectors {
    static final class Manager extends DistanceManager {
        final Map<Long,ChunkHolder> holders=new HashMap<>();
        final List<String> changes=new ArrayList<>();
        Manager(TicketStorage tickets){super(tickets,Runnable::run,Runnable::run);}
        protected boolean isChunkToRemove(long key){return false;}
        protected ChunkHolder getChunk(long key){return holders.get(key);}
        protected ChunkHolder updateChunkScheduling(long key,int level,ChunkHolder holder,int oldLevel){
            var pos=ChunkPos.unpack(key);
            changes.add(pos.x()+","+pos.z()+","+level);
            if(level>ChunkLevel.MAX_LEVEL){holders.remove(key);return null;}
            if(holder==null){
                holder=new ChunkHolder(pos,level,LevelHeightAccessor.create(0,16),null,(p,old,l,set)->{},(p,b)->List.of());
                holders.put(key,holder);
            }else{holder.setTicketLevel(level);}
            return holder;
        }
        void print(String stage){System.out.println(stage+"\t"+String.join(";",changes));changes.clear();}
    }
    public static void main(String[] args){
        var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
        System.out.println("max-loaded-level\t"+ChunkLevel.MAX_LEVEL);
        var tickets=new TicketStorage();var manager=new Manager(tickets);var tracker=new LoadingChunkTracker(manager,tickets);
        var a=new ChunkPos(0,0);var b=new ChunkPos(2,-1);
        tickets.addTicket(new Ticket(TicketType.PLAYER_LOADING,31),a);tracker.runDistanceUpdates(Integer.MAX_VALUE);manager.print("origin");
        tickets.addTicket(new Ticket(TicketType.PLAYER_LOADING,32),b);tracker.runDistanceUpdates(Integer.MAX_VALUE);manager.print("overlap");
        tickets.removeTicket(new Ticket(TicketType.PLAYER_LOADING,31),a);tracker.runDistanceUpdates(Integer.MAX_VALUE);manager.print("remove-origin");
        tickets.removeTicket(new Ticket(TicketType.PLAYER_LOADING,32),b);tracker.runDistanceUpdates(Integer.MAX_VALUE);manager.print("empty");
        sourceUpdates();
    }
    static void sourceUpdates(){
        var tickets=new TicketStorage();var updates=new ArrayList<String>();
        tickets.setLoadingChunkUpdatedListener((key,level,down)->updates.add(update("loading",key,level,down)));
        tickets.setSimulationChunkUpdatedListener((key,level,down)->updates.add(update("simulation",key,level,down)));
        var a=new ChunkPos(0,0);
        tickets.addTicket(new Ticket(TicketType.PLAYER_SIMULATION,30),a);
        tickets.addTicket(new Ticket(TicketType.PLAYER_LOADING,31),a);
        tickets.addTicket(new Ticket(TicketType.FORCED,29),a);
        tickets.addTicket(new Ticket(TicketType.FORCED,29),a);
        tickets.addTicket(new Ticket(TicketType.FORCED,32),a);
        tickets.removeTicket(new Ticket(TicketType.FORCED,32),a);
        tickets.removeTicket(new Ticket(TicketType.FORCED,29),a);
        tickets.removeTicket(new Ticket(TicketType.FORCED,29),a);
        tickets.removeTicket(new Ticket(TicketType.PLAYER_SIMULATION,30),a);
        tickets.removeTicket(new Ticket(TicketType.PLAYER_LOADING,31),a);
        System.out.println("ticket-add-remove\t"+String.join(";",updates));updates.clear();
        for(int i=0;i<24;i++){
            var pos=new ChunkPos(i%6-3,i/6-2);
            tickets.addTicket(new Ticket(TicketType.FORCED,30),pos);
            tickets.addTicket(new Ticket(TicketType.PLAYER_LOADING,31),pos);
            if(i%3==0)tickets.addTicket(new Ticket(TicketType.PLAYER_SIMULATION,32),pos);
        }
        updates.clear();
        tickets.removeTicketIf((t,pos)->t.getType()!=TicketType.PLAYER_SIMULATION,null);
        System.out.println("ticket-purge\t"+String.join(";",updates));
    }
    static String update(String kind,long key,int level,boolean down){
        var pos=ChunkPos.unpack(key);return kind+","+pos.x()+","+pos.z()+","+level+","+down;
    }
}
