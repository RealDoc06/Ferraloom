import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.util.ProblemReporter;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Actual attached/unattached SpawnerBlockEntity load callbacks and saved state. */
public final class SpawnerLoadVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  static final String SEED="{Delay:81s,SpawnData:{entity:{id:'pig'}},SpawnPotentials:[{weight:5,data:{entity:{id:'cow'}}}],MinSpawnDelay:301,MaxSpawnDelay:901,SpawnCount:9,MaxNearbyEntities:8,RequiredPlayerRange:12,SpawnRange:7,components:{'minecraft:custom_name':'old'}}";
  public static void main(String[] args) throws Exception {
    var out=System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
    var world=NeighborVectors.load();
    var builtin=RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),world.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var inputs=List.of("{}", "{SpawnData:{entity:{id:'zombie'}}}", "{Delay:27s,SpawnData:{entity:{}},MinSpawnDelay:55,SpawnCount:2,SpawnPotentials:[]}", "{SpawnData:{}}", "{SpawnData:7,SpawnCount:3}", "{SpawnData:{entity:{id:'invalid identifier'}}}", "{SpawnData:{entity:{id:'skeleton'}},SpawnPotentials:[{weight:2,data:{entity:{id:'cow'}}}],components:{'minecraft:max_stack_size':7}}", "{SpawnData:{entity:{id:'pig'},custom_spawn_rules:{block_light_limit:99}},MinSpawnDelay:90}");
    for(boolean attached:new boolean[]{false,true}) for(var current:List.of(Blocks.SPAWNER.defaultBlockState(),Blocks.STONE.defaultBlockState())) for(boolean custom:new boolean[]{false,true}) for(var input:inputs) {
      var entity=new SpawnerBlockEntity(POS,Blocks.SPAWNER.defaultBlockState());
      entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,context,TagParser.parseCompoundFully(SEED)));
      var level=mock(ServerLevel.class); when(level.registryAccess()).thenReturn(world); when(level.getBlockState(POS)).thenReturn(current);
      var notifications=new ArrayList<String>();
      doAnswer(c->{ notifications.add(Block.getId(c.getArgument(1))+","+Block.getId(c.getArgument(2))+","+c.getArgument(3)); return null; }).when(level).sendBlockUpdated(eq(POS),any(),any(),anyInt());
      if(attached)entity.setLevel(level);
      var tag=TagValueInput.create(ProblemReporter.DISCARDING,context,TagParser.parseCompoundFully(input));
      if(custom)entity.loadCustomOnly(tag);else entity.loadWithComponents(tag);
      System.out.println("spawner-live\t"+attached+"\t"+Block.getId(current)+"\t"+custom+"\t"+input+"\t"+entity.saveWithFullMetadata(context)+"\t"+String.join(";",notifications));
    }
  }
}
