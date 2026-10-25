import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.gameevent.vibrations.VibrationSystem;
import net.minecraft.world.level.storage.TagValueInput;
import static org.mockito.Mockito.*;

/** Real sculk factories/codecs; listener/user mechanics are never simulated. */
public final class SculkDataVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  static HolderLookup.Provider context;
  static String flags(BlockEntity entity){
    if(!(entity instanceof VibrationSystem vibration))return "-";
    var data=vibration.getVibrationData();return data.shouldReloadVibrationParticle()+","+data.getTravelTimeInTicks()+","+(data.getCurrentVibration()!=null);
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();WorldgenBlockEntityVectors.registries=registries;var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level=mock(Level.class);doReturn(false).when(level).isClientSide();
    String event="{game_event:'minecraft:step',distance:3.5f,pos:[1.25d,-2.5d,3.75d],source:[I;1,2,3,4],projectile_owner:[I;5,6,7,8]}";
    var vibrationInputs=new ArrayList<String>(List.of(
      "{}","{last_vibration_frequency:12,warning_level:3}","{last_vibration_frequency:'bad',warning_level:-17}","{listener:{}}","{listener:{selector:{}}}","{listener:{selector:{tick:7L}}}","{listener:{selector:{tick:-1L},event_delay:37}}","{listener:{selector:{tick:0L},event_delay:-1}}","{listener:{selector:{tick:0L},event_delay:'bad'}}",
      "{listener:{event:"+event+",selector:{tick:-1L},event_delay:9}}",
      "{listener:{selector:{event:"+event+",tick:123L},event_delay:5}}",
      "{listener:{event:"+event+",selector:{event:"+event+",tick:-10L},event_delay:0}}",
      "{listener:{event:"+event+",selector:{tick:'bad'},event_delay:3}}",
      "{listener:{event:{game_event:'minecraft:unknown_event',distance:1f,pos:[0d,1d,2d]},selector:{tick:1L}}}",
      "{listener:{event:{game_event:'step',distance:-1f,pos:[0d,1d,2d]},selector:{tick:1L}}}",
      "{listener:{event:{game_event:'step',distance:0f,pos:[0d,1d]},selector:{tick:1L}}}",
      "{listener:{event:{game_event:'step',distance:-0.0f,pos:[0d,1d,2d],source:'bad',projectile_owner:[I;1]},selector:{tick:1L}}}",
      "{components:{'minecraft:custom_name':'sculk','minecraft:max_stack_size':7},listener:{selector:{tick:8L}},warning_level:4,last_vibration_frequency:15}",
      "{listener:{event:{},selector:{event:{},tick:42L},event_delay:4}}",
      "{listener:'bad',warning_level:4294967301L,last_vibration_frequency:2.75f}"
    ));
    var cursorInputs=new ArrayList<String>(List.of(
      "{}","{cursors:[]}","{cursors:[{pos:[I;1,2,3]}]}",
      "{cursors:[{pos:[I;1,2,3],charge:1000,decay_delay:0,update_delay:2147483647,facings:['east','down','east','up']}]}",
      "{cursors:[{pos:[1,2,3],charge:-1,decay_delay:-1,update_delay:-1,facings:[]}]}",
      "{cursors:[{pos:[I;1,2,3],charge:1001,decay_delay:2,update_delay:'bad',facings:['north','invalid']}]}",
      "{cursors:[{pos:[I;1,2,3],charge:17},{} ,{pos:[I;4,5,6],charge:31,facings:[]}]}",
      "{cursors:[{pos:[I;1,2]}]}","{cursors:'bad'}",
      "{components:{'minecraft:custom_name':'catalyst'},cursors:[{pos:[I;7,8,9],charge:0,decay_delay:1,update_delay:0,facings:['west','south']}]}",
      "{cursors:[{pos:[I;1,2,3],charge:'bad',decay_delay:'bad',facings:'bad'}]}"
    ));
    for(int size:new int[]{32,33,34}){
      var cursors=new ArrayList<String>();for(int i=0;i<size;i++)cursors.add("{pos:[I;"+i+",2,3],charge:"+i+"}");cursorInputs.add("{cursors:["+String.join(",",cursors)+"]}");
    }
    var mixed=new ArrayList<String>();mixed.add("{}");for(int i=0;i<32;i++)mixed.add("{pos:[I;"+i+",2,3],charge:17}");cursorInputs.add("{cursors:["+String.join(",",mixed)+"]}");
    for(var block:List.of(Blocks.SCULK_SENSOR,Blocks.CALIBRATED_SCULK_SENSOR,Blocks.SCULK_SHRIEKER,Blocks.SCULK_CATALYST)){
      for(String input:block==Blocks.SCULK_CATALYST?cursorInputs:vibrationInputs){
        var entity=((EntityBlock)block).newBlockEntity(POS,block.defaultBlockState());
        entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,context,TagParser.parseCompoundFully(input)));
        var saved=entity.saveWithFullMetadata(context);var again=BlockEntity.loadStatic(POS,block.defaultBlockState(),saved,context);
        System.out.println("sculk\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+input+"\t"+saved+"\t"+entity.getUpdateTag(context)+"\t"+(entity.getUpdatePacket()!=null)+"\t"+DataComponentMap.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE),entity.collectComponents()).getOrThrow()+"\t"+flags(entity)+"\t"+again.saveWithFullMetadata(context)+"\t"+flags(again));
      }
      var stackTag=TagParser.parseCompoundFully("{id:'"+BuiltInRegistries.ITEM.getKey(block.asItem())+"',count:1,components:{'minecraft:custom_name':'placed','minecraft:max_stack_size':7,'minecraft:block_entity_data':{id:'"+BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(((EntityBlock)block).newBlockEntity(POS,block.defaultBlockState()).getType())+"',warning_level:3,last_vibration_frequency:12}}}");
      var stack=ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE),stackTag).getOrThrow();var entity=((EntityBlock)block).newBlockEntity(POS,block.defaultBlockState());entity.applyComponentsFromItemStack(stack);
      System.out.println("sculk-components\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+stackTag+"\t"+entity.saveWithFullMetadata(context)+"\t"+DataComponentMap.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE),entity.collectComponents()).getOrThrow()+"\t"+flags(entity));
      var fixture=new WorldgenBlockEntityVectors.Fixture(block,WorldgenBlockEntityVectors.initial("DUMMY"));fixture.region.getBlockEntity(POS);
      System.out.println("sculk-worldgen\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+fixture.saved());
    }
  }
}
