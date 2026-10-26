import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.blockscan.BoxBlockMatcher;
import net.minecraft.world.level.chunk.*;
import static org.mockito.Mockito.*;

/** Native Beehive setChanged class predicate, inclusive fire scan and empty release. */
public final class BeehiveChangedVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();var factory=PalettedContainerFactory.create(registries);
    var offsets=new ArrayList<BlockPos>();for(int x=-1;x<=1;x++)for(int y=-1;y<=1;y++)for(int z=-1;z<=1;z++)offsets.add(new BlockPos(x,y,z));
    for(var direction:Direction.values())offsets.add(BlockPos.ZERO.relative(direction,2));
    offsets.add(new BlockPos(99,0,0));offsets.add(new BlockPos(98,0,0));offsets.add(new BlockPos(97,0,0));
    for(var offset:offsets){
      boolean attached=offset.getX()!=97;var placed=offset.getX()==99?Blocks.SOUL_FIRE:offset.getX()==98?Blocks.AIR:Blocks.FIRE;
      var target=POS.offset(offset.getX()>=97?BlockPos.ZERO:offset);
      var sections=new LevelChunkSection[6];for(int y=0;y<6;y++)sections[y]=new LevelChunkSection(factory);
      sections[target.getY()>>4].setBlockState(target.getX()&15,target.getY()&15,target.getZ()&15,placed.defaultBlockState());
      var level=mock(ServerLevel.class);var chunk=mock(LevelChunk.class);var trace=new ArrayList<String>();
      doReturn(0).when(level).getMinY();doReturn(95).when(level).getMaxY();doReturn(0).when(level).getMinSectionY();doReturn(5).when(level).getMaxSectionY();
      doReturn(chunk).when(level).getChunk(anyInt(),anyInt());doAnswer(i->i.getArgument(0)).when(chunk).getSectionIndexFromSectionY(anyInt());
      doAnswer(i->sections[(int)i.getArgument(0)]).when(chunk).getSection(anyInt());
      doAnswer(i->new BoxBlockMatcher(level,i.getArgument(0),i.getArgument(1))).when(level).findBlocksIn(any(BlockPos.class),any(BlockPos.class));
      doReturn(Blocks.BEEHIVE.defaultBlockState()).when(level).getBlockState(POS);
      doAnswer(i->{trace.add("changed");return null;}).when(level).blockEntityChanged(any());
      doAnswer(i->{trace.add("output");return null;}).when(level).updateNeighbourForOutputSignal(any(),any());
      var entity=new BeehiveBlockEntity(POS,Blocks.BEEHIVE.defaultBlockState());if(attached)entity.setLevel(level);
      boolean nearby=entity.isFireNearby();entity.setChanged();
      var actualOffset=target.subtract(POS);
      System.out.println("beehive-changed\t"+actualOffset.getX()+"\t"+actualOffset.getY()+"\t"+actualOffset.getZ()+"\t"+Block.getId(placed.defaultBlockState())+"\t"+attached+"\t"+nearby+"\t"+String.join(";",trace));
    }
  }
}
