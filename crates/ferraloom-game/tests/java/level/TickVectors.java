import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.ticks.*;

/** Executes the real 26.3 LevelTicks and LevelChunkTicks implementations. */
public final class TickVectors {
    private static final String TYPE = "block";
    private static final BlockPos A = new BlockPos(0, 0, 0), B = new BlockPos(1, 0, 0), C = new BlockPos(16, 0, 0);
    private static String name(BlockPos pos) { return pos.equals(A) ? "A" : pos.equals(B) ? "B" : "C"; }
    public static void main(String[] args) {
        var ticks = new LevelTicks<String>(pos -> true);
        ticks.addContainer(new ChunkPos(0,0), new LevelChunkTicks<>());
        ticks.addContainer(new ChunkPos(1,0), new LevelChunkTicks<>());
        ticks.schedule(new ScheduledTick<>(TYPE,A,1,TickPriority.LOW,0));
        ticks.schedule(new ScheduledTick<>(TYPE,B,2,TickPriority.EXTREMELY_HIGH,1));
        ticks.schedule(new ScheduledTick<>(TYPE,C,3,TickPriority.NORMAL,2));
        ticks.schedule(new ScheduledTick<>(TYPE,A,0,TickPriority.EXTREMELY_HIGH,3));
        var order = new ArrayList<String>();
        ticks.tick(3,1,(pos,type) -> order.add(name(pos)));
        ticks.tick(3,65536,(pos,type) -> order.add(name(pos)));
        System.out.println("container-order\t" + String.join(",",order));

        ticks.schedule(new ScheduledTick<>(TYPE,A,4,TickPriority.NORMAL,4));
        ticks.schedule(new ScheduledTick<>(TYPE,B,4,TickPriority.NORMAL,5));
        order.clear();
        ticks.tick(4,65536,(pos,type) -> {
            order.add(name(pos) + ":" + ticks.willTickThisTick(B,TYPE));
            if(pos.equals(A)) ticks.schedule(new ScheduledTick<>(TYPE,C,4,TickPriority.NORMAL,6));
        });
        ticks.tick(4,65536,(pos,type) -> order.add(name(pos)));
        System.out.println("reentrant-collection\t" + String.join(",",order));

        var saved = new LevelChunkTicks<String>(List.of(new SavedTick<>(TYPE,A,5,TickPriority.NORMAL),new SavedTick<>(TYPE,B,2,TickPriority.HIGH)));
        saved.schedule(new ScheduledTick<>(TYPE,A,0,TickPriority.EXTREMELY_HIGH,0));
        saved.unpack(10);
        order.clear();
        for(var tick : saved.pack(10)) order.add(name(tick.pos())+":"+tick.delay()+":"+tick.priority().getValue());
        System.out.println("saved-pack\t" + String.join(",",order));
        order.clear();
        while(saved.peek()!=null) { var tick=saved.poll(); order.add(name(tick.pos())+":"+tick.triggerTick()+":"+tick.subTickOrder()); }
        System.out.println("saved-unpack\t" + String.join(",",order));
    }
}
