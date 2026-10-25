import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.RandomizableContainer;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.loot.LootTable;
import static org.mockito.Mockito.*;

/** Native brushable load/save/update and explicit seed assignment; transient
 * counters are set before load to prove that native loading preserves them. */
public final class BrushableVectors {
  static void counters(BrushableBlockEntity entity)throws Exception {
    for(var name:List.of("brushCount","brushCountResetsAtTick","coolDownEndsAtTick")){
      var field=BrushableBlockEntity.class.getDeclaredField(name);field.setAccessible(true);
      if(name.equals("brushCount"))field.setInt(entity,5);else field.setLong(entity,name.equals("brushCountResetsAtTick")?311:83);
    }
  }
  static String counterValues(BrushableBlockEntity entity)throws Exception {
    var out=new ArrayList<String>();for(var name:List.of("brushCount","brushCountResetsAtTick","coolDownEndsAtTick")){var field=BrushableBlockEntity.class.getDeclaredField(name);field.setAccessible(true);out.add(field.get(entity).toString());}return String.join(",",out);
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();WorldgenBlockEntityVectors.registries=registries;var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    var pos=WorldgenBlockEntityVectors.POS;var table=ResourceKey.create(Registries.LOOT_TABLE,Identifier.withDefaultNamespace("archaeology/desert_well"));
    var inputs=List.of("{}","{item:{id:'minecraft:diamond',count:3},hit_direction:5}","{item:{id:'stone',count:0},hit_direction:500}","{item:{id:'air',count:1},hit_direction:-7}","{item:{id:'stone',count:100},hit_direction:'bad'}","{LootTable:'archaeology/desert_well',LootTableSeed:0L,item:{id:'diamond',count:3},hit_direction:2}","{LootTable:'archaeology/desert_well',LootTableSeed:-123L,hit_direction:-128}","{LootTable:'INVALID NAME',LootTableSeed:37L,item:{id:'stone'},hit_direction:7}","{LootTable:5,LootTableSeed:'bad',item:{id:'stone',components:{'minecraft:custom_name':'found'}},hit_direction:128}","{brushCount:9,brushCountResetsAtTick:999L,coolDownEndsAtTick:999L,hit_direction:2147483647}");
    var level=mock(Level.class);when(level.registryAccess()).thenReturn(registries);
    for(var block:List.of(Blocks.SUSPICIOUS_SAND,Blocks.SUSPICIOUS_GRAVEL))for(String input:inputs)for(String action:List.of("load","set-zero","set-position")){
      var raw=TagParser.parseCompoundFully(input);var entity=(BrushableBlockEntity)((EntityBlock)block).newBlockEntity(pos,block.defaultBlockState());counters(entity);
      entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,registries,raw));
      entity.setLevel(level);clearInvocations(level);
      if(!action.equals("load")){entity.setLootTable(table,action.equals("set-zero")?0:pos.asLong());verifyNoInteractions(level);}
      var saved=entity.saveWithFullMetadata(registries);var again=(BrushableBlockEntity)BlockEntity.loadStatic(pos,block.defaultBlockState(),saved,registries);
      System.out.println("brushable\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+action+"\t"+raw+"\t"+saved+"\t"+entity.getUpdateTag(registries)+"\t"+again.getUpdateTag(registries)+"\t"+counterValues(entity)+"\t"+(entity.getUpdatePacket()!=null));
    }
    for(var block:List.of(Blocks.SUSPICIOUS_SAND,Blocks.SUSPICIOUS_GRAVEL))for(String action:List.of("random","zero","position")){
      var f=new WorldgenBlockEntityVectors.Fixture(block,WorldgenBlockEntityVectors.initial("DUMMY"));
      var random=new net.minecraft.world.level.levelgen.LegacyRandomSource(263777);
      if(action.equals("random"))RandomizableContainer.setBlockEntityLootTable(f.region,random,pos,table);
      else ((BrushableBlockEntity)f.region.getBlockEntity(pos)).setLootTable(table,action.equals("zero")?0:pos.asLong());
      System.out.println("brushable-worldgen\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+action+"\t"+f.saved()+"\t"+random.nextLong());
    }
  }
}
