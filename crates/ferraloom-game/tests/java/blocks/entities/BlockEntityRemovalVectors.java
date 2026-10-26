import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.Vec3;
import net.minecraft.sounds.*;
import static org.mockito.Mockito.*;

/** Real LevelChunk removal/replacement ordering and real class setRemoved. */
public final class BlockEntityRemovalVectors {
  static final BlockPos POS=FactoryBatchTwoVectors.POS;
  @SuppressWarnings({"rawtypes","unchecked"})
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var data=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var registries=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),data.listRegistries()));FactoryBatchTwoVectors.context=registries;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(DataComponentInitializers.PendingComponents::apply);
    for(var block:List.of(Blocks.BEACON,Blocks.JUKEBOX))for(String mode:List.of("break","replace","clear")){
      var state=block.defaultBlockState();var entity=((EntityBlock)block).newBlockEntity(POS,state);
      FactoryBatchTwoVectors.load(entity,block==Blocks.JUKEBOX?"{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:7L}":"{CustomName:'old'}");
      var receivers=new LinkedHashMap<BlockPos,BlockEntity>();receivers.put(POS,entity);
      var level=mock(ServerLevel.class,RETURNS_DEEP_STUBS);doReturn(-64).when(level).getMinY();doReturn(false).when(level).isClientSide();doReturn(state).when(level).getBlockState(POS);
      entity.setLevel(level);var trace=new ArrayList<String>();var observed=new ArrayList<String>();
      doAnswer(call->{var pos=(BlockPos)call.getArgument(1);var sound=(SoundEvent)call.getArgument(2);trace.add("s:"+sound.location()+":"+(pos.getX()+0.5)+","+(pos.getY()+0.5)+","+(pos.getZ()+0.5)+":blocks:"+call.getArgument(4)+":"+call.getArgument(5));observed.add(entity.isRemoved()+","+(receivers.get(POS)==entity));return null;}).when(level).playSound(isNull(),eq(POS),any(SoundEvent.class),eq(SoundSource.BLOCKS),anyFloat(),anyFloat());
      doAnswer(call->{var event=(Holder<GameEvent>)call.getArgument(0);var context=(GameEvent.Context)call.getArgument(2);trace.add("g:"+BuiltInRegistries.GAME_EVENT.getId(event.value())+":8.5,64.5,8.5:"+Block.getId(context.affectedState()));observed.add(entity.isRemoved()+","+(receivers.get(POS)==entity));return null;}).when(level).gameEvent(any(Holder.class),eq(POS),any(GameEvent.Context.class));
      doAnswer(call->{trace.add("l:"+call.getArgument(0)+":8,64,8:"+call.getArgument(2));return null;}).when(level).levelEvent(anyInt(),eq(POS),anyInt());
      var chunk=mock(LevelChunk.class,CALLS_REAL_METHODS);FactoryBatchTwoVectors.set(chunk,"level",level);FactoryBatchTwoVectors.set(chunk,"blockEntities",receivers);FactoryBatchTwoVectors.set(chunk,"tickersInLevel",new HashMap<>());FactoryBatchTwoVectors.set(chunk,"loaded",true);doReturn(state).when(chunk).getBlockState(POS);
      if(mode.equals("replace"))chunk.setBlockEntity(((EntityBlock)block).newBlockEntity(POS,state));
      else if(mode.equals("clear"))chunk.clearAllBlockEntities();
      else chunk.removeBlockEntity(POS);
      var current=receivers.get(POS);
      System.out.println("be-remove\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+mode+"\t"+String.join(";",trace)+"\t"+entity.isRemoved()+"\t"+(current==null?"none":current.isRemoved())+"\t"+entity.saveCustomOnly(registries)+"\t"+String.join(";",observed));
    }
  }
}
