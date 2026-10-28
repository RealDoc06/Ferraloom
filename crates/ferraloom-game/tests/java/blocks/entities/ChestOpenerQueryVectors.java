import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.CompoundContainer;
import net.minecraft.world.entity.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.entity.*;
import net.minecraft.world.phys.AABB;
import static org.mockito.Mockito.*;

/** Actual ContainerOpenersCounter predicate and Level/section entity query. */
public final class ChestOpenerQueryVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    for(double range:List.of(0.0,4.5,8.0))for(double offset:List.of(-9.5,-4.5,0.5,5.5,10.0,13.5))for(String identity:List.of("same","compound_first","compound_second","other","wrong_menu"))for(boolean spectator:List.of(false,true)){
      var chest=new ChestBlockEntity(POS,net.minecraft.core.registries.BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("copper_chest")).defaultBlockState());
      var other=new ChestBlockEntity(POS,net.minecraft.core.registries.BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("copper_chest")).defaultBlockState());
      var counter=(ContainerOpenersCounter)FactoryBatchTwoVectors.get(chest,"openersCounter");
      FactoryBatchTwoVectors.set(counter,"maxInteractionRange",range);
      var sections=new EntitySectionStorage<Entity>(Entity.class,ignored->Visibility.TICKING);
      var getter=new LevelEntityGetterAdapter<Entity>(new EntityLookup<Entity>(),sections);
      var level=mock(ServerLevel.class,i->{
        if(i.getMethod().getName().equals("getEntities"))return i.getArguments().length==0?getter:i.callRealMethod();
        if(i.getMethod().getName().equals("dragonParts"))return List.of();
        return org.mockito.Answers.RETURNS_DEFAULTS.answer(i);
      });
      chest.setLevel(level);var player=mock(ServerPlayer.class);
      doReturn(player).when(player).getLivingEntity();doReturn(spectator).when(player).isSpectator();
      doCallRealMethod().when(player).hasContainerOpen(any(),any());
      double x=POS.getX()+offset;var box=new AABB(x-0.5,64,8,x+0.5,65,9);doReturn(box).when(player).getBoundingBox();
      if(identity.equals("wrong_menu"))player.containerMenu=mock(InventoryMenu.class);
      else {
        var menu=mock(ChestMenu.class);player.containerMenu=menu;
        doReturn(switch(identity){case "same"->chest;case "compound_first"->new CompoundContainer(chest,other);case "compound_second"->new CompoundContainer(other,chest);default->other;}).when(menu).getContainer();
      }
      sections.getOrCreateSection(SectionPos.asLong(BlockPos.containing(x,64,8))).add(player);
      var result=chest.getEntitiesWithContainerOpen();
      System.out.println("chest-opener-query\t"+range+"\t"+offset+"\t"+identity+"\t"+spectator+"\t"+result.size());
    }
    var chest=new ChestBlockEntity(POS,Blocks.CHEST.defaultBlockState());
    var counter=(ContainerOpenersCounter)FactoryBatchTwoVectors.get(chest,"openersCounter");
    var trace=new ArrayList<String>();var level=mock(ServerLevel.class);chest.setLevel(level);
    doReturn(net.minecraft.util.RandomSource.create(0)).when(level).getRandom();
    doAnswer(i->{trace.add("event:"+i.getArgument(2)+":"+i.getArgument(3));return null;}).when(level).blockEvent(any(),any(),anyInt(),anyInt());
    var player=mock(ServerPlayer.class);
    int step=0;
    for(double range:List.of(3.0,8.0,2.0)){
      trace.clear();counter.incrementOpeners(player,level,POS,chest.getBlockState(),range);
      System.out.println("chest-opener-counter\t"+(step++)+"\tstart\t"+range+"\t"+counter.getOpenerCount()+"\t"+FactoryBatchTwoVectors.get(counter,"maxInteractionRange")+"\t"+String.join(";",trace));
    }
    for(int n=0;n<3;n++){
      trace.clear();counter.decrementOpeners(player,level,POS,chest.getBlockState());
      System.out.println("chest-opener-counter\t"+(step++)+"\tstop\t0.0\t"+counter.getOpenerCount()+"\t"+FactoryBatchTwoVectors.get(counter,"maxInteractionRange")+"\t"+String.join(";",trace));
    }
  }
}
