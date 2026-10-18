import java.lang.reflect.*;
import java.util.*;
import java.util.concurrent.Executor;
import net.minecraft.SharedConstants;
import net.minecraft.core.SectionPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;

/** Real PlayerTicketTracker and ThrottlingChunkTaskDispatcher, with deterministic
 * executor polling points. No holder future is completed implicitly. */
public final class PlayerLoadingVectors {
    static final class Queue implements Executor {
        final ArrayDeque<Runnable> tasks=new ArrayDeque<>();
        public void execute(Runnable r){tasks.add(r);}
        void drain(){while(!tasks.isEmpty())tasks.remove().run();}
    }
    static final class Manager extends DistanceManager {
        Manager(TicketStorage tickets,Queue dispatcher,Queue main){super(tickets,dispatcher,main);}
        protected boolean isChunkToRemove(long key){throw new UnsupportedOperationException();}
        protected ChunkHolder getChunk(long key){throw new UnsupportedOperationException();}
        protected ChunkHolder updateChunkScheduling(long key,int level,ChunkHolder chunk,int oldLevel){throw new UnsupportedOperationException();}
    }
    static Object field(Object object,Class<?> type,String name)throws Exception{
        var f=type.getDeclaredField(name);f.setAccessible(true);return f.get(object);
    }
    static void pump(Queue dispatcher,Queue main){
        while(!dispatcher.tasks.isEmpty()||!main.tasks.isEmpty()){dispatcher.drain();main.drain();}
    }
    static void print(String stage,List<String> rows){System.out.println(stage+"\t"+String.join(";",rows));rows.clear();}
    public static void main(String[] args)throws Exception{
        var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
        var tickets=new TicketStorage();var dispatcher=new Queue();var main=new Queue();
        var manager=new Manager(tickets,dispatcher,main);var trace=new ArrayList<String>();
        tickets.setLoadingChunkUpdatedListener((key,level,down)->{
            var p=ChunkPos.unpack(key);trace.add(p.x()+","+p.z()+","+level+","+down);
        });
        var tracker=field(manager,DistanceManager.class,"playerTicketManager");
        var run=tracker.getClass().getDeclaredMethod("runAllUpdates");run.setAccessible(true);
        var view=tracker.getClass().getDeclaredMethod("updateViewDistance",int.class);view.setAccessible(true);
        var throttle=(ThrottlingChunkTaskDispatcher)field(manager,DistanceManager.class,"ticketDispatcher");
        var a=SectionPos.of(0,0,0);var b=SectionPos.of(2,0,-1);
        view.invoke(tracker,2);manager.addPlayer(a,null);run.invoke(tracker);pump(dispatcher,main);print("origin",trace);
        // Successful entity-ticking futures release these exact slots.
        for(var p:List.of(new ChunkPos(0,0),new ChunkPos(-1,-1),new ChunkPos(1,1),new ChunkPos(-2,-2))){
            throttle.release(p.pack(),()->{},false);pump(dispatcher,main);print("release-"+p.x()+","+p.z(),trace);
        }
        manager.removePlayer(a,null);manager.addPlayer(b,null);run.invoke(tracker);pump(dispatcher,main);print("move",trace);
        view.invoke(tracker,0);pump(dispatcher,main);print("shrink",trace);
        manager.removePlayer(b,null);run.invoke(tracker);pump(dispatcher,main);print("empty",trace);
    }
}
