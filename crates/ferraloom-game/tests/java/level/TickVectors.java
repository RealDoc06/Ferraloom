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

        var delayed = new LevelChunkTicks<String>(List.of(new SavedTick<>(TYPE,A,7,TickPriority.NORMAL)));
        var dimension = new LevelTicks<String>(pos -> true);
        dimension.addContainer(new ChunkPos(0,0),delayed);
        order.clear();
        dimension.tick(100,65536,(pos,type)->order.add(name(pos)+"@100"));
        System.out.println("registered-pending\t"+delayed.pack(200).getFirst().delay()+":"+order.size());
        dimension.schedule(new ScheduledTick<>(TYPE,B,105,TickPriority.NORMAL,0));
        delayed.unpack(200);
        delayed.unpack(500);
        dimension.tick(200,65536,(pos,type)->order.add(name(pos)+"@200"));
        dimension.tick(206,65536,(pos,type)->order.add(name(pos)+"@206"));
        dimension.tick(207,65536,(pos,type)->order.add(name(pos)+"@207"));
        System.out.println("late-start-ticking\t"+String.join(",",order));

        var equal = new LevelTicks<String>(pos -> true);
        for(int x=-10;x<=10;x++){
            var p=new BlockPos(x*16,0,0);var container=new LevelChunkTicks<String>(List.of(new SavedTick<>(TYPE,p,0,TickPriority.NORMAL)));container.unpack(0);equal.addContainer(new ChunkPos(x,0),container);
        }
        order.clear();for(int pass=0;pass<5;pass++)equal.tick(0,5,(pos,type)->order.add(""+pos.getX()));
        System.out.println("equal-loaded-heads\t"+String.join(",",order));
        var area = new LevelTicks<String>(pos -> true);
        for(int x=0;x<=3;x++)area.addContainer(new ChunkPos(x,0),new LevelChunkTicks<>());
        area.schedule(new ScheduledTick<>(TYPE,A,0,TickPriority.NORMAL,0));area.schedule(new ScheduledTick<>(TYPE,B,0,TickPriority.NORMAL,1));area.schedule(new ScheduledTick<>(TYPE,C,0,TickPriority.NORMAL,2));
        area.schedule(new ScheduledTick<>(TYPE,new BlockPos(2,0,0),9,TickPriority.NORMAL,3));
        order.clear();
        area.tick(0,65536,(pos,type)->{
            order.add(""+pos.getX());
            if(pos.equals(A)){
                order.add("before:"+area.willTickThisTick(B,TYPE));
                area.copyArea(new net.minecraft.world.level.levelgen.structure.BoundingBox(0,0,0,16,0,0),new net.minecraft.core.Vec3i(32,0,0));
                area.clearArea(new net.minecraft.world.level.levelgen.structure.BoundingBox(0,0,0,1,0,0));
                order.add("after:"+area.willTickThisTick(B,TYPE));
            }
        });
        area.tick(0,65536,(pos,type)->order.add(""+pos.getX()));area.tick(9,65536,(pos,type)->order.add(""+pos.getX()));
        System.out.println("area-copy-clear\t"+String.join(",",order));
    }
}
