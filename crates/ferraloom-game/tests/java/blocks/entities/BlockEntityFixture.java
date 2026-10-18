import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.TickRateManager;
import net.minecraft.world.level.block.entity.*;

/** Real Level + LevelChunk storage. Only absent external services are replaced. */
class BlockEntityFixture extends MutationFixture {
  final TickRateManager clock=new TickRateManager();
  BlockEntityFixture(RegistryAccess r){super(r);}
  @Override public BlockEntity getBlockEntity(BlockPos pos){return getChunkAt(pos).getBlockEntity(pos);}
  @Override public TickRateManager tickRateManager(){return clock;}
  @Override public boolean shouldTickBlocksAt(BlockPos pos){return true;}
  @Override public void blockEntityChanged(BlockPos pos){}
  static void tickerPhases(RegistryAccess r){
    var level=new BlockEntityFixture(r);var events=new ArrayList<String>();
    class Ticker implements TickingBlockEntity {
      final String name;boolean removed;Runnable action=()->{};
      Ticker(String name){this.name=name;}
      public boolean isRemoved(){return removed;}
      public BlockPos getPos(){return BlockPos.ZERO;}
      public String getType(){return name;}
      public void tick(){events.add(name);action.run();}
    }
    var a=new Ticker("a");var b=new Ticker("b");var c=new Ticker("c");
    a.action=()->{b.removed=true;level.addBlockEntityTicker(c);a.action=()->{};};
    level.addBlockEntityTicker(a);level.addBlockEntityTicker(b);
    level.tickBlockEntities();System.out.println("ticker-phase\tfirst\t"+String.join(",",events));events.clear();
    level.tickBlockEntities();System.out.println("ticker-phase\tsecond\t"+String.join(",",events));events.clear();
    level.clock.setFrozen(true);level.clock.tick();a.removed=true;
    level.tickBlockEntities();System.out.println("ticker-phase\tfrozen\t"+String.join(",",events));
  }
}
