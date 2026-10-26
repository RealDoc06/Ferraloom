import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import static org.mockito.Mockito.*;

/** Native typed data/load/component and attachment boundaries, without ticking. */
public final class FactoryBatchThreeVectors {
  static final BlockPos POS=FactoryBatchTwoVectors.POS;
  static HolderLookup.Provider context;
  static void prime(BlockEntity entity)throws Exception {
    if(entity instanceof BeaconBlockEntity){FactoryBatchTwoVectors.set(entity,"levels",7);FactoryBatchTwoVectors.set(entity,"lastCheckY",91);}
    if(entity instanceof ConduitBlockEntity){FactoryBatchTwoVectors.set(entity,"tickCount",91);FactoryBatchTwoVectors.set(entity,"activeRotation",1.25f);FactoryBatchTwoVectors.set(entity,"isActive",true);FactoryBatchTwoVectors.set(entity,"isHunting",true);FactoryBatchTwoVectors.set(entity,"nextAmbientSoundActivation",123L);((List<BlockPos>)FactoryBatchTwoVectors.get(entity,"effectBlocks")).add(POS.above());}
    if(entity instanceof ShelfBlockEntity shelf){shelf.getItems().set(0,new ItemStack(Items.DIAMOND,3));FactoryBatchTwoVectors.set(entity,"alignItemsToBottom",true);}
    if(entity instanceof JukeboxBlockEntity jukebox){FactoryBatchTwoVectors.set(entity,"item",new ItemStack(Items.DIAMOND,3));FactoryBatchTwoVectors.set(jukebox.getSongPlayer(),"ticksSinceSongStarted",91L);}
  }
  static String scalar(BlockEntity entity)throws Exception {
    if(entity instanceof BeaconBlockEntity)return FactoryBatchTwoVectors.get(entity,"levels")+","+FactoryBatchTwoVectors.get(entity,"lastCheckY");
    if(entity instanceof ConduitBlockEntity)return FactoryBatchTwoVectors.get(entity,"tickCount")+","+Float.floatToRawIntBits((float)FactoryBatchTwoVectors.get(entity,"activeRotation"))+","+FactoryBatchTwoVectors.get(entity,"isActive")+","+FactoryBatchTwoVectors.get(entity,"isHunting")+","+((List<?>)FactoryBatchTwoVectors.get(entity,"effectBlocks")).size()+","+FactoryBatchTwoVectors.get(entity,"nextAmbientSoundActivation");
    if(entity instanceof ShelfBlockEntity shelf)return Boolean.toString(shelf.getAlignItemsToBottom());
    if(entity instanceof JukeboxBlockEntity jukebox){var player=jukebox.getSongPlayer();var song=FactoryBatchTwoVectors.get(player,"song");return (song==null?"null":((Holder<?>)song).unwrapKey().orElseThrow().identifier())+","+player.getTicksSinceSongStarted()+","+jukebox.getComparatorOutput();}
    return "base";
  }
  @SuppressWarnings({"rawtypes","unchecked"})
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var data=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),data.listRegistries())); FactoryBatchTwoVectors.context=context;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level=mock(ServerLevel.class);doReturn(false).when(level).isClientSide();doReturn(-64).when(level).getMinY();doReturn(data).when(level).registryAccess();
    var states=new ArrayList<BlockState>();
    for(var block:BuiltInRegistries.BLOCK)if(block instanceof ShelfBlock || block instanceof CopperGolemStatueBlock)states.add(block.defaultBlockState());
    states.add(Blocks.BEACON.defaultBlockState());states.add(Blocks.CONDUIT.defaultBlockState());
    for(boolean record:List.of(false,true))states.add(Blocks.JUKEBOX.defaultBlockState().setValue(JukeboxBlock.HAS_RECORD,record));
    for(var state:states){
      var block=(EntityBlock)state.getBlock();var inputs=new ArrayList<String>(List.of("@new","{}","{CustomName:'named',lock:{},components:{'minecraft:custom_name':'extra','minecraft:max_stack_size':7}}"));
      if(block instanceof BeaconBlock)inputs.addAll(List.of("{Levels:4,primary_effect:'speed',secondary_effect:'regeneration',CustomName:'light',lock:{items:'minecraft:stone'}}","{primary_effect:'poison',secondary_effect:'test:missing',Levels:2}","{primary_effect:1,secondary_effect:'haste',CustomName:7,lock:'bad'}"));
      if(block instanceof ConduitBlock)inputs.addAll(List.of("{Target:[I;0,1,2,3]}","{Target:[1,2,3,4]}","{Target:[I;1,2,3]}","{Target:'00000000-0000-0001-0000-000200000003'}","{Target:[I;1,2,3,4,5]}","{Target:'bad'}"));
      if(block instanceof ShelfBlock)inputs.addAll(List.of("{Items:[{Slot:0b,id:'stone',count:0},{Slot:1b,id:'book',count:100},{Slot:2b,id:'diamond',count:3},{Slot:3b,id:'dirt',count:5}],align_items_to_bottom:1b}","{Items:[{Slot:1b,id:'stone',count:7},{Slot:1b,id:'book',count:1}],align_items_to_bottom:0b}","{Items:[1,2],align_items_to_bottom:'bad'}"));
      if(block instanceof JukeboxBlock)inputs.addAll(List.of(
        "{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:0L}","{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:3579L}","{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:3580L}","{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:-1L}","{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:9223372036854775807L}",
        "{RecordItem:{id:'music_disc_13',count:0},ticks_since_song_started:2L}","{RecordItem:{id:'music_disc_13',count:100},ticks_since_song_started:2L}",
        "{RecordItem:{id:'stone',count:1,components:{'minecraft:jukebox_playable':'minecraft:cat'}},ticks_since_song_started:7L}",
        "{RecordItem:{id:'music_disc_13',count:1,components:{'minecraft:jukebox_playable':'test:missing'}},ticks_since_song_started:7L}",
        "{RecordItem:{id:'stone',count:1,components:{'minecraft:jukebox_playable':'test:missing'}},ticks_since_song_started:7L}",
        "{RecordItem:{id:'test:missing',count:1},ticks_since_song_started:3L}","{RecordItem:'bad',ticks_since_song_started:'bad'}"));
      for(String input:inputs){
        var entity=block.newBlockEntity(POS,state);if(!input.equals("@new")){prime(entity);FactoryBatchTwoVectors.load(entity,input);}
        var saved=entity.saveWithFullMetadata(context);var round=BlockEntity.loadStatic(POS,state,saved,context);entity.setLevel(level);
        System.out.println("factory3-data\t"+Block.getId(state)+"\t"+input+"\t"+saved+"\t"+entity.getUpdateTag(context)+"\t"+(entity.getUpdatePacket()!=null)+"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+scalar(entity)+"\t"+(block.getTicker(level,state,(BlockEntityType)entity.getType())!=null)+"\t"+round.saveWithFullMetadata(context));
      }
      for(String contents:List.of("[]","[{slot:0,item:{id:'stone',count:7}},{slot:2,item:{id:'book',count:2}},{slot:3,item:{id:'diamond',count:5}}]")){
        var entity=block.newBlockEntity(POS,state);prime(entity);
        String input="{id:'"+BuiltInRegistries.ITEM.getKey(state.getBlock().asItem())+"',count:1,components:{'minecraft:custom_name':'placed','minecraft:lock':{items:'minecraft:stone'},'minecraft:container':"+contents+"}}";
        var item=ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE),TagParser.parseCompoundFully(input)).getOrThrow();entity.applyComponentsFromItemStack(item);
        var stripped=net.minecraft.world.level.storage.TagValueOutput.createWithContext(net.minecraft.util.ProblemReporter.DISCARDING,context);entity.saveCustomOnly(stripped);entity.removeComponentsFromTag(stripped);
        System.out.println("factory3-components\t"+Block.getId(state)+"\t"+input+"\t"+entity.saveWithFullMetadata(context)+"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+stripped.buildResult());
      }
    }
    for(int min:List.of(-2032,-64,0)){
      var beacon=new BeaconBlockEntity(POS,Blocks.BEACON.defaultBlockState());doReturn(min).when(level).getMinY();beacon.setLevel(level);
      System.out.println("beacon-attach\t"+min+"\t"+FactoryBatchTwoVectors.get(beacon,"lastCheckY"));
    }
  }
}
