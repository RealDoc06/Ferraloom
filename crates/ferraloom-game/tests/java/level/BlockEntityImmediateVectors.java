import java.util.*;
import java.util.stream.Stream;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.chunk.*;
import static org.mockito.Mockito.*;

/** Actual CHECK/IMMEDIATE promotion, on-demand creation and removed-entry lookup. */
public final class BlockEntityImmediateVectors {
  static final BlockPos POS=new BlockPos(1,1,1);
  static final String LIVE="{id:'minecraft:chest',x:1,y:1,z:1,LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:17L}";
  static String saved(BlockEntity entity,HolderLookup.Provider context){return entity==null?"-":entity.saveWithFullMetadata(context).toString();}
  static String pending(LevelChunk chunk){var tag=chunk.getBlockEntityNbt(POS);return tag==null?"-":tag.toString();}
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(p->p.apply());
    for(var mode:List.of(LevelChunk.EntityCreationType.CHECK,LevelChunk.EntityCreationType.IMMEDIATE))for(String kind:List.of("missing","stone","dummy","real","unknown","wrong_type","dummy_stone","live","live_pending","removed","removed_real","removed_dummy","missing_furnace","dummy_furnace")){
      var level=mock(ServerLevel.class);when(level.registryAccess()).thenReturn(registries);when(level.palettedContainerFactory()).thenReturn(PalettedContainerFactory.create(registries));
      when(level.getMinY()).thenReturn(0);when(level.getHeight()).thenReturn(16);when(level.getMaxY()).thenReturn(15);when(level.getSectionsCount()).thenReturn(1);when(level.getMinSectionY()).thenReturn(0);
      when(level.getSectionIndex(anyInt())).thenAnswer(i->((int)i.getArgument(0))>>4);
      var random=RandomSource.create(12345);when(level.getRandom()).thenReturn(random);
      var chunk=new LevelChunk(level,new ChunkPos(0,0));
      var state=kind.contains("stone")?Blocks.STONE.defaultBlockState():kind.contains("furnace")?Blocks.FURNACE.defaultBlockState():Blocks.CHEST.defaultBlockState();
      chunk.getSection(0).setBlockState(1,1,1,state);
      String raw=switch(kind){case "dummy","dummy_stone","removed_dummy","dummy_furnace"->"{id:'DUMMY',x:1,y:1,z:1}";case "real","removed_real"->LIVE;case "unknown","live_pending"->"{id:'fixture:unbound',x:1,y:1,z:1,opaque:17}";case "wrong_type"->"{id:'minecraft:sign',x:1,y:1,z:1}";default->null;};
      if(raw!=null)chunk.setBlockEntityNbt(TagParser.parseCompoundFully(raw));
      BlockEntity old=null;
      if(kind.startsWith("live")||kind.startsWith("removed")){old=BlockEntity.loadStatic(POS,state,TagParser.parseCompoundFully(LIVE),registries);chunk.setBlockEntity(old);if(kind.startsWith("removed"))old.setRemoved();}
      var first=chunk.getBlockEntity(POS,mode);String firstSaved=saved(first,context),firstPending=pending(chunk);boolean firstStored=chunk.getBlockEntities().containsKey(POS);
      var second=chunk.getBlockEntity(POS,mode);
      System.out.println("immediate\t"+kind+"\t"+Block.getId(state)+"\t"+mode+"\t"+(raw==null?"-":raw)+"\t"+firstSaved+"\t"+firstPending+"\t"+firstStored+"\t"+saved(second,context)+"\t"+pending(chunk)+"\t"+chunk.getBlockEntities().containsKey(POS)+"\t"+(first!=null&&first==second)+"\t"+(old!=null&&old==first)+"\t"+(old!=null&&old==second)+"\t"+random.nextLong());
    }
  }
}
