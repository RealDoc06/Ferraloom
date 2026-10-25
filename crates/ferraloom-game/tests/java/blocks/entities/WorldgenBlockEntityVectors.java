import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.util.*;
import net.minecraft.world.RandomizableContainer;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;
import net.minecraft.world.level.levelgen.feature.treedecorators.BeehiveDecorator;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Native WorldGenRegion#getBlockEntity on actual ProtoChunk storage. Mocks
 * supply the enclosing server and chunk lookup, never block-entity behavior. */
public final class WorldgenBlockEntityVectors {
  static RegistryAccess registries;
  static final BlockPos POS=new BlockPos(8,64,8);
  static final ResourceKey<net.minecraft.world.level.storage.loot.LootTable> TABLE=ResourceKey.create(Registries.LOOT_TABLE,Identifier.withDefaultNamespace("chests/simple_dungeon"));
  static final Method NEXT;
  static final Method BEES;
  static {
    try {
      NEXT=BaseSpawner.class.getDeclaredMethod("getOrCreateNextSpawnData",Level.class,RandomSource.class,BlockPos.class);NEXT.setAccessible(true);
      BEES=Arrays.stream(BeehiveDecorator.class.getDeclaredMethods()).filter(m->Arrays.equals(m.getParameterTypes(),new Class<?>[]{RandomSource.class,BeehiveBlockEntity.class})).findFirst().orElseThrow();BEES.setAccessible(true);
    }catch(Exception e){throw new ExceptionInInitializerError(e);}
  }
  static class Fixture {
    final ServerLevel level=mock(ServerLevel.class);
    final ProtoChunk chunk=new ProtoChunk(new ChunkPos(0,0),UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),null);
    final WorldGenRegion region=mock(WorldGenRegion.class,CALLS_REAL_METHODS);
    Fixture(Block block,CompoundTag pending)throws Exception {
      when(level.registryAccess()).thenReturn(registries);
      when(level.palettedContainerFactory()).thenReturn(PalettedContainerFactory.create(registries));
      when(level.getMinY()).thenReturn(-64);when(level.getHeight()).thenReturn(384);
      when(level.getSectionsCount()).thenReturn(24);when(level.getMinSectionY()).thenReturn(-4);
      when(level.getSectionIndex(anyInt())).thenAnswer(c->(((int)c.getArgument(0))>>4)+4);
      chunk.setPersistedStatus(ChunkStatus.TERRAIN);
      chunk.setBlockState(POS,block.defaultBlockState(),0);
      if(pending!=null)chunk.setBlockEntityNbt(pending.copy());
      var field=WorldGenRegion.class.getDeclaredField("level");field.setAccessible(true);field.set(region,level);
      doReturn(chunk).when(region).getChunk(any(BlockPos.class));
      doReturn(chunk).when(region).getChunk(anyInt(),anyInt());
      doReturn(chunk).when(region).getChunk(anyInt(),anyInt(),any(ChunkStatus.class),anyBoolean());
      doReturn(registries).when(region).registryAccess();
      doAnswer(c->{
        BlockPos p=c.getArgument(0);var state=(net.minecraft.world.level.block.state.BlockState)c.getArgument(1);
        chunk.setBlockState(p,state,0);
        chunk.removeBlockEntity(p);
        if(state.hasBlockEntity()){var tag=new CompoundTag();tag.putString("id","DUMMY");tag.putInt("x",p.getX());tag.putInt("y",p.getY());tag.putInt("z",p.getZ());chunk.setBlockEntityNbt(tag);}
        return true;
      }).when(region).setBlock(any(BlockPos.class),any(net.minecraft.world.level.block.state.BlockState.class),anyInt());
    }
    CompoundTag saved(){return chunk.getBlockEntityNbtForSaving(POS,registries);}
  }
  static CompoundTag tag(String text)throws Exception{return TagParser.parseCompoundFully(text);}
  static CompoundTag initial(String id)throws Exception{return tag("{id:'"+id+"',x:8,y:64,z:8}");}
  static void sample(String action,Block block,long seed,CompoundTag pending,CompoundTag incoming)throws Exception {
    var f=new Fixture(block,pending);var random=new net.minecraft.world.level.levelgen.WorldgenRandom(new net.minecraft.world.level.levelgen.LegacyRandomSource(seed));
    switch(action){
      case "loot" -> RandomizableContainer.setBlockEntityLootTable(f.region,random,POS,TABLE);
      case "seed" -> {var entity=f.region.getBlockEntity(POS);if(entity instanceof RandomizableContainer container)container.setLootTable(TABLE,-9123456789L);}
      case "spawner", "spawner-twice" -> {var entity=f.region.getBlockEntity(POS);if(entity instanceof SpawnerBlockEntity spawner){spawner.setEntityId(EntityTypes.ZOMBIE,random);if(action.equals("spawner-twice"))spawner.setEntityId(EntityTypes.SKELETON,random);}}
      case "bees" -> {var entity=f.region.getBlockEntity(POS);if(entity instanceof BeehiveBlockEntity hive)BEES.invoke(null,random,hive);}
      case "load" -> {
        var template=new StructureTemplate();
        var constructor=StructureTemplate.Palette.class.getDeclaredConstructor(List.class);constructor.setAccessible(true);
        var palette=constructor.newInstance(List.of(new StructureTemplate.StructureBlockInfo(BlockPos.ZERO,block.defaultBlockState(),incoming.copy())));
        var field=StructureTemplate.class.getDeclaredField("palettes");field.setAccessible(true);((List<StructureTemplate.Palette>)field.get(template)).add(palette);
        field=StructureTemplate.class.getDeclaredField("size");field.setAccessible(true);field.set(template,new Vec3i(1,1,1));
        template.placeInWorld(f.region,POS,POS,new StructurePlaceSettings().setIgnoreEntities(true).setKnownShape(true),random,2);
      }
      default -> throw new IllegalArgumentException(action);
    }
    var entity=f.region.getBlockEntity(POS);boolean same=entity==f.region.getBlockEntity(POS);
    System.out.println(String.join("\t","worldgen",action,BuiltInRegistries.BLOCK.getKey(block).toString(),Long.toString(seed),pending==null?"-":pending.toString(),incoming==null?"-":incoming.toString(),String.valueOf(f.saved()),Long.toString(random.nextLong()),Boolean.toString(!f.chunk.getBlockEntityNbts().isEmpty()),Boolean.toString(same)));
  }
  static void promotion(long seed)throws Exception {
    var raw=tag("{id:'minecraft:mob_spawner',x:8,y:64,z:8,SpawnPotentials:[{weight:1,data:{entity:{id:'minecraft:zombie'}}},{weight:3,data:{entity:{id:'minecraft:skeleton'}}}]}");
    var f=new Fixture(Blocks.SPAWNER,raw);
    var before=(SpawnerBlockEntity)f.region.getBlockEntity(POS);
    var prep=new net.minecraft.world.level.levelgen.LegacyRandomSource(seed);
    before.setEntityId(EntityTypes.ZOMBIE,prep); // nextSpawnData now aliases one potential.
    var saved=before.saveWithFullMetadata(registries);
    var full=new LevelChunk(f.level,f.chunk,null);
    var original=(SpawnerBlockEntity)full.getBlockEntity(POS);
    if(before!=original)throw new AssertionError("native promotion replaced receiver");
    var reloaded=(SpawnerBlockEntity)BlockEntity.loadStatic(POS,Blocks.SPAWNER.defaultBlockState(),saved,registries);
    var nativeRandom=new net.minecraft.world.level.levelgen.LegacyRandomSource(seed+1);var loadedRandom=new net.minecraft.world.level.levelgen.LegacyRandomSource(seed+1);
    var first=(SpawnData)NEXT.invoke(original.getSpawner(),null,nativeRandom,POS);
    var roundtripFirst=(SpawnData)NEXT.invoke(reloaded.getSpawner(),null,loadedRandom,POS);
    if(!first.getEntityToSpawn().equals(roundtripFirst.getEntityToSpawn())||nativeRandom.nextLong()!=loadedRandom.nextLong())throw new AssertionError("first selection changed");
    var firstEntity=first.getEntityToSpawn().copy();
    original.setEntityId(EntityTypes.CREEPER,nativeRandom);reloaded.setEntityId(EntityTypes.CREEPER,loadedRandom);
    System.out.println("promotion\t"+seed+"\t"+raw+"\t"+saved+"\t"+firstEntity+"\t"+original.saveWithFullMetadata(registries)+"\t"+reloaded.saveWithFullMetadata(registries)+"\t"+nativeRandom.nextLong()+"\t"+loadedRandom.nextLong());
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    registries=NeighborVectors.load();
    var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    for(long seed:new long[]{0,1,-1,9123456}){
      for(var block:List.of(Blocks.CHEST,Blocks.TRAPPED_CHEST,Blocks.COPPER_CHEST.asList().getFirst(),Blocks.BARREL,Blocks.SHULKER_BOX,Blocks.DISPENSER,Blocks.DROPPER,Blocks.HOPPER,Blocks.FURNACE,Blocks.OAK_SIGN,Blocks.SPAWNER,Blocks.BEE_NEST,Blocks.MOVING_PISTON,Blocks.AIR))for(String action:List.of("loot","seed"))sample(action,block,seed,initial("DUMMY"),null);
      sample("loot",Blocks.CHEST,seed,null,null);
      sample("loot",Blocks.CHEST,seed,initial("minecraft:invalid"),null);
      for(var raw:List.of(initial("DUMMY"),initial("minecraft:mob_spawner"),tag("{id:'minecraft:mob_spawner',x:8,y:64,z:8,SpawnData:{entity:{id:'zombie'}},SpawnPotentials:[{weight:1,data:{entity:{id:'skeleton'}}}]}"),tag("{id:'minecraft:mob_spawner',x:8,y:64,z:8,SpawnPotentials:[{weight:1,data:{entity:{id:'zombie'}}},{weight:3,data:{entity:{id:'skeleton'}}}]}")))for(String action:List.of("spawner","spawner-twice"))sample(action,Blocks.SPAWNER,seed,raw,null);
      sample("spawner",Blocks.CHEST,seed,initial("DUMMY"),null);
      for(var block:List.of(Blocks.BEE_NEST,Blocks.BEEHIVE,Blocks.CHEST))sample("bees",block,seed,initial("DUMMY"),null);
      sample("load",Blocks.CHEST,seed,null,tag("{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:17L,components:{'minecraft:custom_name':'\"native\"'}}"));
      sample("load",Blocks.SPAWNER,seed,null,tag("{SpawnData:{entity:{id:'zombie'}},SpawnCount:7,SpawnPotentials:[]}"));
      sample("load",Blocks.BEE_NEST,seed,null,tag("{bees:[{entity_data:{id:'minecraft:bee'},ticks_in_hive:31,min_ticks_in_hive:600}],flower_pos:[I;1,2,3]}"));
      promotion(seed);
    }
  }
}
