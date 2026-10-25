import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import static org.mockito.Mockito.*;

/** Typed constructors, EntityBlock factories and loadStatic are intentionally
 * distinct; ticker selection uses the real bootstrapped dimension types. */
public final class FactoryTickerVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();
    var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    var level=mock(Level.class);when(level.isClientSide()).thenReturn(false);
    for(var state:Blocks.CRAFTER.getStateDefinition().getPossibleStates()) {
      for(String path:List.of("typed","factory","missing","zero","one","two","bad")) {
        CompoundTag input=new CompoundTag();input.putString("id","minecraft:crafter");
        switch(path){case "zero"->input.putInt("triggered",0);case "one"->input.putInt("triggered",1);case "two"->input.putInt("triggered",2);case "bad"->input.putString("triggered","bad");}
        BlockEntity entity=switch(path){case "typed"->BlockEntityTypes.CRAFTER.create(POS,state);case "factory"->((EntityBlock)Blocks.CRAFTER).newBlockEntity(POS,state);default->BlockEntity.loadStatic(POS,state,input,registries);};
        boolean ticker=((EntityBlock)Blocks.CRAFTER).getTicker(level,state,BlockEntityTypes.CRAFTER)!=null;
        System.out.println("factory\t"+Block.BLOCK_STATE_REGISTRY.getId(state)+"\t"+path+"\t"+input+"\t"+entity.saveWithFullMetadata(registries)+"\t"+ticker);
      }
    }
    var dimensions=registries.lookupOrThrow(Registries.DIMENSION_TYPE);
    for(var name:List.of("overworld","the_nether","the_end")) {
      var type=dimensions.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(name));
      when(level.dimensionType()).thenReturn(type);
      for(var state:Blocks.DAYLIGHT_DETECTOR.getStateDefinition().getPossibleStates()) {
        var entity=((EntityBlock)Blocks.DAYLIGHT_DETECTOR).newBlockEntity(POS,state);
        boolean ticker=((EntityBlock)Blocks.DAYLIGHT_DETECTOR).getTicker(level,state,BlockEntityTypes.DAYLIGHT_DETECTOR)!=null;
        System.out.println("daylight\t"+name+"\t"+type.hasSkyLight()+"\t"+Block.BLOCK_STATE_REGISTRY.getId(state)+"\t"+ticker+"\t"+entity.saveWithFullMetadata(registries));
      }
    }
  }
}
