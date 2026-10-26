import java.util.*;
import static org.mockito.Mockito.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.ItemStackWithSlot;
import net.minecraft.world.inventory.PlayerEnderChestContainer;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.EnderChestBlockEntity;
import net.minecraft.world.level.storage.*;

/** Actual PlayerEnderChestContainer typed load/save and change notifications. */
public final class EnderVectors {
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    var registries=NeighborVectors.load();
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var provider=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(provider).forEach(pending->pending.apply());
    String[] inputs={
      "{}", "{EnderItems:[]}",
      "{EnderItems:[{Slot:26b,id:'minecraft:stone',count:3},{Slot:0b,id:'minecraft:diamond',count:2}]}",
      "{EnderItems:[{id:'minecraft:stone',count:77}]}",
      "{EnderItems:[{Slot:1b,id:'minecraft:stone',count:2},{Slot:1b,id:'minecraft:diamond',count:3}]}",
      "{EnderItems:[{Slot:27b,id:'minecraft:stone',count:1},{Slot:-1b,id:'minecraft:diamond',count:1}]}" ,
      "{EnderItems:[{Slot:'invalid',id:'minecraft:stone',count:1}]}" ,
      "{EnderItems:[{Slot:257,id:'minecraft:stone',count:3},{Slot:-256,id:'minecraft:diamond',count:4}]}" ,
      "{EnderItems:[{Slot:26.9d,id:'minecraft:stone',count:4}]}" ,
      "{EnderItems:[{Slot:2b,id:'minecraft:missing_item',count:1},{Slot:3b,id:'minecraft:stone',count:4}]}"
    };
    for(int i=0;i<inputs.length;i++) {
      var container=spy(new PlayerEnderChestContainer());container.setItem(0,new ItemStack(Items.DIAMOND,5));container.setItem(26,new ItemStack(Items.STONE,3));
      var active=new EnderChestBlockEntity(BlockPos.ZERO,Blocks.ENDER_CHEST.defaultBlockState());container.setActiveChest(active);
      int[] changes={0};doAnswer(call->{changes[0]++;return call.callRealMethod();}).when(container).setChanged();
      container.fromSlots(TagValueInput.create(ProblemReporter.DISCARDING,registries,TagParser.parseCompoundFully(inputs[i])).listOrEmpty("EnderItems",ItemStackWithSlot.CODEC));
      var output=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,registries);
      container.storeAsSlots(output.list("EnderItems",ItemStackWithSlot.CODEC));
      System.out.println("ender-nbt\t"+i+"\t"+inputs[i]+"\t"+output.buildResult()+"\t"+changes[0]+"\t"+container.isActiveChest(active));
    }
  }
}
