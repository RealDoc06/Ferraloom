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
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Native gateway state codecs, portalTick and cooldown block-event receiver. */
public final class EndGatewayFactoryVectors {
  static final BlockPos POS = FactoryBatchTwoVectors.POS;
  static HolderLookup.Provider context;
  static void prime(TheEndGatewayBlockEntity entity) throws Exception {
    FactoryBatchTwoVectors.set(entity,"age",77L);
    FactoryBatchTwoVectors.set(entity,"teleportCooldown",7);
    entity.setExitPosition(new BlockPos(1,2,3),true);
  }
  static String scalar(TheEndGatewayBlockEntity entity) throws Exception {
    return FactoryBatchTwoVectors.get(entity,"age") + "," + FactoryBatchTwoVectors.get(entity,"teleportCooldown")
      + "," + FactoryBatchTwoVectors.get(entity,"exitPortal") + "," + FactoryBatchTwoVectors.get(entity,"exactTeleport")
      + "," + entity.isSpawning() + "," + entity.isCoolingDown();
  }
  public static void main(String[] args) throws Exception {
    var output=System.out;
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
    var data=NeighborVectors.load();
    var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),data.listRegistries()));
    FactoryBatchTwoVectors.context=context;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var state=Blocks.END_GATEWAY.defaultBlockState();
    var level=mock(ServerLevel.class);doReturn(data).when(level).registryAccess();doReturn(false).when(level).isClientSide();
    var inputs=List.of("@new","{}",
      "{Age:123L,exit_portal:[I;1,2,3],ExactTeleport:1b,components:{'minecraft:custom_name':'extra','minecraft:max_stack_size':7}}",
      "{Age:'bad',exit_portal:'bad',ExactTeleport:'bad'}",
      "{Age:-1.25f,ExactTeleport:256}","{Age:-1.25d,ExactTeleport:-1.25f}",
      "{exit_portal:[I;1,2]}","{exit_portal:[I;1,2,3,4]}",
      "{exit_portal:[1,2,3,'bad']}","{exit_portal:[1.25f,-1.25f,3.5f]}",
      "{exit_portal:[B;1b,2b,3b]}","{exit_portal:[L;4294967297L,2L,3L]}",
      "{exit_portal:[I;-30000000,0,0]}","{exit_portal:[I;-30000001,0,0]}",
      "{exit_portal:[I;29999999,0,0]}","{exit_portal:[I;30000000,0,0]}",
      "{exit_portal:[I;0,0,-30000000]}","{exit_portal:[I;0,0,30000000]}",
      "{exit_portal:[I;0,-20000000,0]}","{exit_portal:[I;0,-20000001,0]}",
      "{exit_portal:[I;0,19999999,0]}","{exit_portal:[I;0,20000000,0]}");
    for(String input:inputs){
      var entity=new TheEndGatewayBlockEntity(POS,state);
      if(!input.equals("@new")){prime(entity);FactoryBatchTwoVectors.load(entity,input);}
      entity.setLevel(level);
      var saved=entity.saveWithFullMetadata(context);
      var round=BlockEntity.loadStatic(POS,state,saved,context);
      System.out.println("gateway-data\t"+input+"\t"+saved+"\t"+entity.getUpdateTag(context)+"\t"+(entity.getUpdatePacket()!=null)
        +"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+scalar(entity)+"\t"+round.saveWithFullMetadata(context));
    }
    var entity=new TheEndGatewayBlockEntity(POS,state);prime(entity);
    String itemTag="{id:'stone',count:1,components:{'minecraft:custom_name':'placed','minecraft:custom_data':{value:3}}}";
    entity.applyComponentsFromItemStack(ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE),TagParser.parseCompoundFully(itemTag)).getOrThrow());
    System.out.println("gateway-components\t"+itemTag+"\t"+entity.saveWithFullMetadata(context)+"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+scalar(entity));
    for(long age:new long[]{-1,0,198,199,200,2398,2399,2400,4799,Long.MAX_VALUE})for(int cooldown:new int[]{-1,0,1,2,40}){
      entity=new TheEndGatewayBlockEntity(POS,state);
      FactoryBatchTwoVectors.set(entity,"age",age);FactoryBatchTwoVectors.set(entity,"teleportCooldown",cooldown);
      var trace=new ArrayList<String>();var tickLevel=mock(ServerLevel.class);doReturn(false).when(tickLevel).isClientSide();
      doAnswer(call->{trace.add("event:"+call.getArgument(2)+":"+call.getArgument(3));return null;}).when(tickLevel).blockEvent(any(),any(),anyInt(),anyInt());
      doAnswer(call->{trace.add("changed");return null;}).when(tickLevel).blockEntityChanged(any());
      doAnswer(call->{trace.add("output");return null;}).when(tickLevel).updateNeighbourForOutputSignal(any(),any());
      entity.setLevel(tickLevel);
      TheEndGatewayBlockEntity.portalTick(tickLevel,POS,state,entity);
      System.out.println("gateway-tick\t"+age+"\t"+cooldown+"\t"+scalar(entity)+"\t"+String.join(";",trace));
    }
    for(int event:new int[]{-1,0,1,2,255}){
      entity=new TheEndGatewayBlockEntity(POS,state);prime(entity);
      boolean handled=entity.triggerEvent(event,-7);
      System.out.println("gateway-event\t"+event+"\t"+handled+"\t"+scalar(entity));
    }
  }
}
