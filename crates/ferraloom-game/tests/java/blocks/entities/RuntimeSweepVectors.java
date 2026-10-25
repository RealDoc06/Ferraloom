import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicReference;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.Nameable;
import net.minecraft.world.attribute.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import static org.mockito.Mockito.*;

/** Independent bootstrap facts and actual block-selected server tick dispatch. */
public final class RuntimeSweepVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var access=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),access.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var registry=new JsonArray();
    for(var type:BuiltInRegistries.BLOCK_ENTITY_TYPE){
      var row=new JsonObject();row.addProperty("id",BuiltInRegistries.BLOCK_ENTITY_TYPE.getId(type));row.addProperty("name",BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type).toString());row.addProperty("only_op_can_set_nbt",type.onlyOpCanSetNbt());
      var blocks=new JsonArray();Boolean nameable=null;
      for(var block:BuiltInRegistries.BLOCK)if(type.isValid(block.defaultBlockState())){
        blocks.add(BuiltInRegistries.BLOCK.getId(block));boolean value=type.create(POS,block.defaultBlockState()) instanceof Nameable;
        if(nameable!=null&&nameable!=value)throw new AssertionError("nameability varies within type");nameable=value;
      }
      row.addProperty("is_nameable",Objects.requireNonNull(nameable));row.add("valid_blocks",blocks);registry.add(row);
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(registry)+"\n");
    var level=mock(Level.class);when(level.isClientSide()).thenReturn(false);
    var changed=new AtomicReference<BlockState>();int[] writes={0};
    doAnswer(call->{BlockState next=call.getArgument(1);boolean differs=changed.getAndSet(next)!=next;if(differs)writes[0]++;return differs;}).when(level).setBlockAndUpdate(eq(POS),any());
    for(boolean crafting:new boolean[]{false,true})for(int remaining:new int[]{0,1,2,Integer.MIN_VALUE,Integer.MAX_VALUE}){
      var state=Blocks.CRAFTER.defaultBlockState().setValue(CrafterBlock.CRAFTING,crafting);changed.set(state);writes[0]=0;
      var entity=new CrafterBlockEntity(POS,state);entity.setCraftingTicksRemaining(remaining);
      var ticker=((EntityBlock)Blocks.CRAFTER).getTicker(level,state,BlockEntityTypes.CRAFTER);Objects.requireNonNull(ticker).tick(level,POS,state,entity);
      int after=entity.saveWithoutMetadata(access).getIntOr("crafting_ticks_remaining",0);
      System.out.println("tick-crafter\t"+Block.getId(state)+"\t"+remaining+"\t"+after+"\t"+Block.getId(changed.get())+"\t"+writes[0]);
    }
    var attributes=mock(EnvironmentAttributeSystem.class);doReturn(0.0f).when(attributes).getValue(EnvironmentAttributes.SUN_ANGLE,POS);doReturn(attributes).when(level).environmentAttributes();
    doReturn(13).when(level).getEffectiveSkyBrightness(POS);
    var dimensions=access.lookupOrThrow(Registries.DIMENSION_TYPE);
    for(var dimension:List.of("overworld","the_nether","the_end")){
      var type=dimensions.getValue(Identifier.withDefaultNamespace(dimension));doReturn(type).when(level).dimensionType();
      for(long time:new long[]{1,20})for(boolean inverted:new boolean[]{false,true}){
        doReturn(time).when(level).getGameTime();var state=Blocks.DAYLIGHT_DETECTOR.defaultBlockState().setValue(DaylightDetectorBlock.POWER,3).setValue(DaylightDetectorBlock.INVERTED,inverted);changed.set(state);writes[0]=0;
        var entity=new DaylightDetectorBlockEntity(POS,state);var ticker=((EntityBlock)Blocks.DAYLIGHT_DETECTOR).getTicker(level,state,BlockEntityTypes.DAYLIGHT_DETECTOR);
        if(ticker!=null)ticker.tick(level,POS,state,entity);
        System.out.println("tick-daylight\t"+Block.getId(state)+"\t"+type.hasSkyLight()+"\t"+time+"\t"+(ticker!=null)+"\t"+Block.getId(changed.get())+"\t"+writes[0]);
      }
    }
  }
}
