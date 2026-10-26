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
import net.minecraft.world.level.block.state.properties.TestBlockMode;
import static org.mockito.Mockito.*;

/** Native TestBlockEntity factory state, transient trigger and component boundaries. */
public final class TestBlockFactoryVectors {
  static void prime(TestBlockEntity entity) throws Exception {
    entity.setMode(TestBlockMode.LOG);entity.setMessage("old");entity.setPowered(true);
    FactoryBatchTwoVectors.set(entity,"triggered",true);
  }
  static String scalar(TestBlockEntity entity) {
    return entity.getMode().getSerializedName()+","+entity.getMessage()+","+entity.isPowered()+","+entity.hasTriggered();
  }
  public static void main(String[] args) throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var data=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),data.listRegistries()));
    FactoryBatchTwoVectors.context=context;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level=mock(ServerLevel.class);doReturn(data).when(level).registryAccess();
    var inputs=List.of("@new","{}","{mode:'start',message:'go',powered:1b}","{mode:'log',message:'line',powered:0b}",
      "{mode:'accept',powered:256}","{mode:'fail',message:'err',powered:-1.25f}","{mode:'bad',message:17,powered:'bad'}",
      "{mode:1,triggered:0b}","{mode:'',powered:-0.25d}","{message:'§alpha',components:{'minecraft:custom_name':'extra','minecraft:max_stack_size':7}}");
    for(var state:Blocks.TEST_BLOCK.getStateDefinition().getPossibleStates()){
      for(String input:inputs){
        var entity=new TestBlockEntity(FactoryBatchTwoVectors.POS,state);
        if(!input.equals("@new")){prime(entity);FactoryBatchTwoVectors.load(entity,input);}
        entity.setLevel(level);var saved=entity.saveWithFullMetadata(context);
        var round=BlockEntity.loadStatic(FactoryBatchTwoVectors.POS,state,saved,context);
        System.out.println("test-block-data\t"+Block.getId(state)+"\t"+input+"\t"+saved+"\t"+entity.getUpdateTag(context)+"\t"+(entity.getUpdatePacket()!=null)
          +"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+scalar(entity)+"\t"+round.saveWithFullMetadata(context));
      }
      var entity=new TestBlockEntity(FactoryBatchTwoVectors.POS,state);prime(entity);
      String tag="{id:'test_block',count:1,components:{'minecraft:custom_name':'placed','minecraft:block_state':{mode:'accept'}}}";
      var item=ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE),TagParser.parseCompoundFully(tag)).getOrThrow();
      entity.applyComponentsFromItemStack(item);
      System.out.println("test-block-components\t"+Block.getId(state)+"\t"+tag+"\t"+entity.saveWithFullMetadata(context)+"\t"+FactoryBatchTwoVectors.components(entity)+"\t"+scalar(entity));
    }
  }
}
