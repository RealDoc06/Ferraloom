import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;

/** ContainerHelper's real typed-list partial reads on chest and furnace data. */
public final class ContainerStorageVectors {
  public static void main(String[] args)throws Exception {
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
    var world=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var registries=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),world.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(DataComponentInitializers.PendingComponents::apply);
    var pos=new BlockPos(8,64,8);
    for(var block:List.of(Blocks.CHEST,Blocks.FURNACE))for(var count:List.of("",",count:0",",count:100",",count:-1",",count:1",",count:99",",count:'bad'",",count:0L")) {
      var tag=TagParser.parseCompoundFully("{Items:[{Slot:0b,id:'minecraft:stone'"+count+",components:{'minecraft:custom_name':'partial'}},{Slot:2b,id:'minecraft:diamond',count:3}]}");
      var entity=((EntityBlock)block).newBlockEntity(pos,block.defaultBlockState());
      entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,registries,tag));
      var saved=entity.saveWithFullMetadata(registries);
      var again=BlockEntity.loadStatic(pos,block.defaultBlockState(),saved,registries);
      System.out.println("container-storage\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+tag+"\t"+saved+"\t"+again.saveWithFullMetadata(registries));
    }
  }
}
