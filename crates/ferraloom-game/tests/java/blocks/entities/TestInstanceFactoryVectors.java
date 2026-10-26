import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import static org.mockito.Mockito.*;

/** Native record-codec partials and loadAdditional -> set -> setChanged order. */
public final class TestInstanceFactoryVectors {
  static final String DATA="size:[I;1,2,3],rotation:'clockwise_90',ignore_entities:1b,status:'running'";
  static TestInstanceBlockEntity create(){return new TestInstanceBlockEntity(FactoryBatchTwoVectors.POS,Blocks.TEST_INSTANCE_BLOCK.defaultBlockState());}
  @SuppressWarnings("unchecked")
  static List<TestInstanceBlockEntity.ErrorMarker> errors(TestInstanceBlockEntity entity)throws Exception{return (List<TestInstanceBlockEntity.ErrorMarker>)FactoryBatchTwoVectors.get(entity,"errorMarkers");}
  static void prime(TestInstanceBlockEntity entity)throws Exception {
    entity.set(new TestInstanceBlockEntity.Data(Optional.of(ResourceKey.create(Registries.TEST_INSTANCE,Identifier.withDefaultNamespace("unregistered"))),new Vec3i(7,8,9),Rotation.CLOCKWISE_180,true,TestInstanceBlockEntity.Status.FINISHED,Optional.of(Component.literal("old"))));
    errors(entity).add(new TestInstanceBlockEntity.ErrorMarker(new BlockPos(4,5,6),Component.literal("old marker")));
  }
  static String visible(TestInstanceBlockEntity entity)throws Exception{return entity.getSize().getX()+":"+errors(entity).size();}
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));FactoryBatchTwoVectors.context=context;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var inputs=new ArrayList<String>(List.of("@new","{}","{data:{}}","{data:{"+DATA+"}}",
      "{data:{"+DATA+",test:'test:unregistered',error_message:'new'}}",
      "{data:{"+DATA+",test:'bad id',error_message:17}}",
      "{data:{size:[I;1,2],rotation:'none',ignore_entities:0b,status:'cleared'}}",
      "{data:{size:[I;1,2,3,4],rotation:'none',ignore_entities:0b,status:'cleared'}}",
      "{data:{size:[1.9f,-2.9f,3.25f],rotation:'none',ignore_entities:0.25f,status:'cleared'}}",
      "{data:{size:[L;4294967300L,2L,3L],rotation:'none',ignore_entities:256,status:'finished'}}",
      "{data:{size:[I;1,2,3],rotation:'wrong',ignore_entities:1b,status:'running'}}",
      "{data:{size:[I;1,2,3],rotation:'none',ignore_entities:'true',status:'running'}}",
      "{data:{size:[I;1,2,3],rotation:'none',ignore_entities:0b,status:'wrong'}}",
      "{data:{size:[I;1,2,3],rotation:'none',ignore_entities:0b}}",
      "{data:{"+DATA+",error_message:{text:'new',extra:[{text:'child'}]}}}",
      "{errors:[{pos:[I;1,2,3],text:'a'},{pos:[I;4,5,6],text:{translate:'test.key'}}]}",
      "{errors:[{pos:[I;1,2],text:'bad'},{pos:[I;4,5,6,7],text:'partial'},{pos:[I;7,8,9],text:17},{pos:[I;10,11,12],text:'last'}]}",
      "{errors:[{pos:[1.75f,-2.75f,3.75f],text:'floats'}]}",
      "{errors:[{}]}","{errors:'bad'}",
      "{data:{"+DATA+"},errors:[{pos:[I;1,2,3],text:'new marker'}],components:{'minecraft:custom_name':'after'}}"));
    for(boolean attached:List.of(false,true))for(String input:inputs){
      var entity=create();var trace=new ArrayList<String>();
      if(!input.equals("@new"))prime(entity);
      if(attached){
        var level=mock(ServerLevel.class);doReturn(registries).when(level).registryAccess();
        doAnswer(i->{trace.add("changed:"+visible(entity));return null;}).when(level).blockEntityChanged(any());
        doAnswer(i->{trace.add("output:"+visible(entity));return null;}).when(level).updateNeighbourForOutputSignal(any(),any());
        doAnswer(i->{trace.add("update:"+Block.getId(i.getArgument(1))+":"+Block.getId(i.getArgument(2))+":"+i.getArgument(3));return null;}).when(level).sendBlockUpdated(any(),any(),any(),anyInt());
        entity.setLevel(level);
      }
      if(!input.equals("@new"))FactoryBatchTwoVectors.load(entity,input);
      var saved=entity.saveWithFullMetadata(context);var round=BlockEntity.loadStatic(FactoryBatchTwoVectors.POS,entity.getBlockState(),saved,context);
      System.out.println("test-instance-data\t"+attached+"\t"+input+"\t"+saved+"\t"+entity.getUpdateTag(context)+"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+String.join(";",trace)+"\t"+round.saveWithFullMetadata(context));
    }
    var entity=create();prime(entity);
    String item="{id:'test_instance_block',count:1,components:{'minecraft:custom_name':'placed'}}";
    entity.applyComponentsFromItemStack(ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE),TagParser.parseCompoundFully(item)).getOrThrow());
    System.out.println("test-instance-components\t"+item+"\t"+entity.saveWithFullMetadata(context)+"\t"+FactoryBatchTwoVectors.components(entity));
  }
}
