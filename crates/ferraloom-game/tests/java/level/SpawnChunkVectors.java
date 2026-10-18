import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.SectionPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.util.RandomSource;
import net.minecraft.util.Util;
import net.minecraft.world.level.*;

/** Real DistanceManager natural-spawn tracker and Util.shuffle. A null player
 * is a set-membership token; no player method or allocation bypass is used. */
public final class SpawnChunkVectors {
    static final class Manager extends DistanceManager {
        Manager(){super(new TicketStorage(),Runnable::run,Runnable::run);}
        protected boolean isChunkToRemove(long key){throw new UnsupportedOperationException();}
        protected ChunkHolder getChunk(long key){throw new UnsupportedOperationException();}
        protected ChunkHolder updateChunkScheduling(long key,int level,ChunkHolder chunk,int oldLevel){throw new UnsupportedOperationException();}
    }
    static void print(Manager manager,String stage) throws Exception {
        var field=DistanceManager.class.getDeclaredField("naturalSpawnChunkCounter");field.setAccessible(true);
        var counter=field.get(manager);
        var getLevel=counter.getClass().getDeclaredMethod("getLevel",long.class);getLevel.setAccessible(true);
        var chunks=new ArrayList<ChunkPos>();var rows=new ArrayList<String>();
        var iterator=manager.getSpawnCandidateChunks();
        while(iterator.hasNext()){
            long key=iterator.nextLong();var pos=ChunkPos.unpack(key);chunks.add(pos);
            rows.add(pos.x()+","+pos.z()+","+getLevel.invoke(counter,key));
        }
        System.out.println(stage+"\t"+String.join(";",rows));
        if(stage.equals("origin")){
            var random=RandomSource.create(0);Util.shuffle(chunks,random);
            System.out.println("shuffled-origin\t"+chunks.stream().map(p->p.x()+","+p.z()).collect(java.util.stream.Collectors.joining(";"))+"|"+random.nextInt());
            System.out.println("missing-level\t"+getLevel.invoke(counter,new ChunkPos(100,100).pack()));
        }
    }
    public static void main(String[] args) throws Exception {
        var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
        var manager=new Manager();
        var a=SectionPos.of(0,0,0);var b=SectionPos.of(2,0,-1);var c=SectionPos.of(-2,0,1);
        manager.addPlayer(a,null);print(manager,"origin");
        manager.addPlayer(b,null);print(manager,"overlap");
        manager.removePlayer(a,null);print(manager,"remove-origin");
        manager.removePlayer(b,null);manager.addPlayer(c,null);print(manager,"move");
        manager.removePlayer(c,null);print(manager,"empty");
    }
}
