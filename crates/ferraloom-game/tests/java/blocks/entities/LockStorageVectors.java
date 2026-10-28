import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;

/** Native LockCode#fromTag map boundaries across real storage/nameable entities. */
public final class LockStorageVectors {
  public static void main(String[] args) throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var data=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),data.listRegistries()));
    FactoryBatchTwoVectors.context=context;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    for(var block:List.of(Blocks.CHEST,Blocks.FURNACE,Blocks.BREWING_STAND,Blocks.BEACON)) {
      var state=block.defaultBlockState();
      for(String lock:List.of("@missing","{}","'bad'","1","[]","1b","{items:'stone'}","{items:'test:missing'}","{count:'bad'}",
        "{items:['stone','test:missing','book'],count:{min:2}}", "{items:'test:missing',count:{min:2}}",
        "{items:'stone',count:'bad'}", "{count:{min:'bad',max:5}}", "{count:{min:7,max:2}}", "{count:-1.25f}",
        "{components:{'minecraft:custom_name':'key','test:missing':1},predicates:{'minecraft:damage':{durability:{min:1}},'test:bad':{}}}",
        "{components:'bad',predicates:{'minecraft:damage':{durability:{min:1}}}}",
        "{components:{'minecraft:custom_name':'key'},predicates:'bad'}", "{items:'#minecraft:planks'}",
        "{items:'#test:missing',count:2}", "{items:[1,2]}", "{items:[]}", "{items:[['stone']]}",
        "{count:{min:-1.25f,max:2.75d}}", "{components:{'minecraft:max_stack_size':0,'minecraft:custom_name':'key'}}")) {
        String input=lock.equals("@missing")?"{}":"{lock:"+lock+"}";
        var entity=((EntityBlock)block).newBlockEntity(FactoryBatchTwoVectors.POS,state);
        FactoryBatchTwoVectors.load(entity,input);
        System.out.println("lock-storage\t"+Block.getId(state)+"\t"+input+"\t"+entity.saveWithFullMetadata(context)+"\t"+FactoryBatchTwoVectors.components(entity));
      }
    }
  }
}
