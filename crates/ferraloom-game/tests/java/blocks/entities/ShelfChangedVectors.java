import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.gameevent.GameEvent;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Actual ShelfBlockEntity superclass, event and update callback order. */
public final class ShelfChangedVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  public static void main(String[] args) {
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
    for(boolean attached:List.of(false,true))for(var cached:List.of(Blocks.OAK_SHELF,Blocks.SPRUCE_SHELF))for(int mode:new int[]{0,1,2})for(boolean mutate:List.of(false,true)) {
      var receiver=new ShelfBlockEntity(POS,Blocks.OAK_SHELF.defaultBlockState());receiver.setBlockState(cached.defaultBlockState());
      var level=mock(ServerLevel.class);var trace=new ArrayList<String>();if(attached)receiver.setLevel(level);
      doAnswer(c->{trace.add("changed");return null;}).when(level).blockEntityChanged(POS);
      doAnswer(c->{trace.add("output:"+Block.getId(receiver.getBlockState()));if(mutate)receiver.setBlockState(Blocks.SPRUCE_SHELF.defaultBlockState());return null;}).when(level).updateNeighbourForOutputSignal(eq(POS),any());
      doAnswer(c->{var context=(GameEvent.Context)c.getArgument(2);trace.add("event:"+((Holder<?>)c.getArgument(0)).unwrapKey().orElseThrow().identifier()+":"+Block.getId(context.affectedState()));return null;}).when(level).gameEvent(org.mockito.ArgumentMatchers.<Holder<GameEvent>>any(),eq(POS),any(GameEvent.Context.class));
      doAnswer(c->{trace.add("update:"+Block.getId(c.getArgument(1))+":"+Block.getId(c.getArgument(2))+":"+c.getArgument(3));return null;}).when(level).sendBlockUpdated(eq(POS),any(),any(),anyInt());
      if(mode==0)receiver.setChanged();else receiver.setChanged(mode==1?null:GameEvent.BLOCK_DEACTIVATE);
      System.out.println("shelf-changed\t"+attached+"\t"+Block.getId(cached.defaultBlockState())+"\t"+mode+"\t"+mutate+"\t"+Block.getId(receiver.getBlockState())+"\t"+String.join(";",trace));
    }
  }
}
