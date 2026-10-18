import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.protocol.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.*;
import net.minecraft.stats.*;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.flag.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.level.redstone.*;
import net.minecraft.world.ticks.*;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real ServerPlaceRecipe clearGrid, real drop receivers and comparator callbacks. */
public final class ClearGridVectors {
  static final BlockPos POS=new BlockPos(8,64,8), COMP=POS.east();
  static final class World extends ServerLevel {
    RecipeManager recipes; MinecraftServer server; GameRules rules;
    Map<BlockPos,BlockState> blocks; Map<BlockPos,BlockEntity> entities;
    LevelTicks<Block> ticks; LevelChunkTicks<Block> chunkTicks;
    List<ItemEntity> drops; List<String> events; int nextId; long subTick;
    World(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public int getNextEntityId(){return ++nextId;}
    @Override public RecipeManager recipeAccess(){return recipes;}
    @Override public MinecraftServer getServer(){return server;}
    @Override public GameRules getGameRules(){return rules;}
    @Override public FeatureFlagSet enabledFeatures(){return FeatureFlags.DEFAULT_FLAGS;}
    @Override public BlockState getBlockState(BlockPos pos){return blocks.getOrDefault(pos,Blocks.AIR.defaultBlockState());}
    @Override public BlockEntity getBlockEntity(BlockPos pos){return entities.get(pos);}
    @Override public boolean hasChunkAt(BlockPos pos){return true;}
    @Override public void blockEntityChanged(BlockPos pos){}
    @Override public LevelTicks<Block> getBlockTicks(){return ticks;}
    @Override public long nextSubTickCount(){return subTick++;}
    @Override public long getGameTime(){return 0;}
    @Override public void scheduleTick(BlockPos pos,Block block,int delay){ticks.schedule(createTick(pos,block,delay));}
    @Override public void scheduleTick(BlockPos pos,Block block,int delay,TickPriority priority){ticks.schedule(createTick(pos,block,delay,priority));}
    @Override public boolean addFreshEntity(Entity entity){
      var item=(ItemEntity)entity;drops.add(item);events.add("add:"+InventoryVectors.item(item.getItem()));
      item.getEntityData().packDirty();return true;
    }
  }
  static final class PlayerFixture extends ServerPlayer {
    List<String> events;
    PlayerFixture(){super(null,null,null,null);}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public boolean swing(InteractionHand hand,net.minecraft.world.item.component.SwingAnimation animation,boolean sendToSelf){return true;}
    @Override public void awardStat(Stat<?> stat,int amount){events.add("stat:"+statName(stat)+":"+amount);}
  }
  static <T> String statName(Stat<T> stat){return stat.getType().getRegistry().getKey(stat.getValue()).toString();}
  static void copyFields(Object from,Object to,Class<?> first)throws Exception {
    for(Class<?> owner=first;owner!=Object.class;owner=owner.getSuperclass())for(Field field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){
      field.setAccessible(true);field.set(to,field.get(from));
    }
  }
  static void scenario(String name,RecipeManager manager,RegistryAccess registries)throws Exception {
    var uf=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");uf.setAccessible(true);var allocator=(sun.misc.Unsafe)uf.get(null);
    var level=(World)allocator.allocateInstance(World.class);
    copyFields(new InventoryVectors.MenuLevel(registries),level,Level.class);
    level.recipes=manager;level.server=mock(MinecraftServer.class);when(level.server.getRecipeManager()).thenReturn(manager);
    level.rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);level.blocks=new HashMap<>();level.entities=new HashMap<>();level.events=new ArrayList<>();level.drops=new ArrayList<>();
    level.ticks=new LevelTicks<>(key->true);level.chunkTicks=new LevelChunkTicks<>();level.ticks.addContainer(new ChunkPos(0,0),level.chunkTicks);
    var neighborField=Level.class.getDeclaredField("neighborUpdater");neighborField.setAccessible(true);neighborField.set(level,new CollectingNeighborUpdater(level,1000000));
    var player=(PlayerFixture)allocator.allocateInstance(PlayerFixture.class);
    copyFields(new InventoryVectors.MenuPlayer(level,false),player,Player.class);
    var inventoryField=Player.class.getDeclaredField("inventory");inventoryField.setAccessible(true);inventoryField.set(player,new Inventory(player,new EntityEquipment()));
    player.events=level.events;player.connection=mock(ServerGamePacketListenerImpl.class);
    doAnswer(call->{var packet=call.getArgument(0);if(packet instanceof ClientboundContainerSetSlotPacket p)level.events.add("S:"+p.getStateId()+":"+p.getSlot()+":"+InventoryVectors.item(p.getItem()));return null;}).when(player.connection).send(any(Packet.class));
    boolean furnace=name.startsWith("furnace");AbstractContainerMenu menu;
    if(furnace){
      var state=Blocks.FURNACE.defaultBlockState();level.blocks.put(POS,state);var entity=new FurnaceBlockEntity(POS,state);entity.setLevel(level);level.entities.put(POS,entity);
      menu=new FurnaceMenu(7,player.getInventory(),entity,new SimpleContainerData(4));
      entity.setItem(0,new ItemStack(Items.IRON_ORE,8));entity.setItem(2,new ItemStack(Items.IRON_INGOT,3));
      if(name.equals("furnace-comparator")){
        var comparator=Blocks.COMPARATOR.defaultBlockState().setValue(ComparatorBlock.FACING,Direction.WEST);
        level.blocks.put(COMP,comparator);level.blocks.put(COMP.below(),Blocks.STONE.defaultBlockState());var be=new ComparatorBlockEntity(COMP,comparator);be.setLevel(level);level.entities.put(COMP,be);
      }
    }else{
      menu=name.equals("table")?new CraftingMenu(7,player.getInventory(),ContainerLevelAccess.create(level,POS)):new InventoryMenu(player.getInventory(),true,player);
      menu.getSlot(1).set(new ItemStack(Items.OAK_LOG,8));
    }
    for(int i=0;i<36;i++)player.getInventory().setItem(i,new ItemStack(Items.DIRT,64));
    if(name.equals("partial"))player.getInventory().setItem(0,new ItemStack(Items.OAK_LOG,60));
    level.events.clear();level.subTick=0;
    var id=Identifier.withDefaultNamespace(furnace?"gold_ingot_from_smelting_raw_gold":"diamond_block");var recipe=manager.byKey(ResourceKey.create(Registries.RECIPE,id)).orElseThrow();
    var action=((RecipeBookMenu)menu).handlePlacement(false,true,recipe,level,player.getInventory());
    var drops=level.drops.stream().map(item->InventoryVectors.item(item.getItem())).toList();
    boolean dirty=level.drops.stream().anyMatch(item->item.getEntityData().packDirty()!=null);
    var inventory=new ArrayList<String>();for(int i=0;i<36;i++)inventory.add(InventoryVectors.item(player.getInventory().getItem(i)));
    System.out.println("clear-grid\t"+name+"\t"+action+"\t"+String.join(";",level.events)+"\t"+InventoryVectors.slots(menu)+"\t"+String.join(";",inventory)+"\t"+String.join(";",drops)+"\t"+dirty+"\t"+level.ticks.hasScheduledTick(COMP,Blocks.COMPARATOR)+":"+level.ticks.count());
  }
  static void run(RecipeManager manager,RegistryAccess registries)throws Exception {
    for(String name:List.of("inventory","table","furnace","partial","furnace-comparator"))scenario(name,manager,registries);
  }
}
