import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.advancements.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.protocol.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.*;
import net.minecraft.stats.*;
import net.minecraft.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real slot listeners, result slots, recipe book and advancement rewards.
 * Nested recipe rewards make criterion order observable in network packets. */
public final class ProducerVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  static final List<String> KEYS=List.of("oak_planks","stick","crafting_table","furnace","diamond_block","emerald_block","torch","iron_ingot_from_smelting_raw_iron");
  static final class World extends ServerLevel {
    RecipeManager recipes; MinecraftServer server; GameRules rules;
    List<String> trace; List<ExperienceOrb> orbs; int nextId=1;
    World(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public int getNextEntityId(){return ++nextId;}
    @Override public RecipeManager recipeAccess(){return recipes;}
    @Override public MinecraftServer getServer(){return server;}
    @Override public GameRules getGameRules(){return rules;}
    @Override public net.minecraft.world.flag.FeatureFlagSet enabledFeatures(){return net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS;}
    @Override public boolean hasChunkAt(BlockPos pos){return true;}
    @Override public long getGameTime(){return 0;}
    @Override public BlockState getBlockState(BlockPos pos){return Blocks.AIR.defaultBlockState();}
    @Override public void blockEntityChanged(BlockPos pos){}
    @Override public <T extends Entity> List<T> getEntitiesOfClass(Class<T> type,AABB box,Predicate<? super T> predicate){
      return orbs.stream().filter(type::isInstance).map(type::cast).filter(e->e.getBoundingBox().intersects(box)).filter(predicate).toList();
    }
    @Override public <T extends Entity> List<T> getEntities(net.minecraft.world.level.entity.EntityTypeTest<Entity,T> type,AABB box,Predicate<? super T> predicate){
      return orbs.stream().map(type::tryCast).filter(Objects::nonNull).filter(e->e.getBoundingBox().intersects(box)).filter(predicate).toList();
    }
    // The fixture's spawn room is AIR and contains only noncollidable XP orbs.
    @Override public boolean noCollision(Entity entity,AABB box){return true;}
    @Override public boolean addFreshEntity(Entity entity){
      var orb=(ExperienceOrb)entity;orbs.add(orb);return true;
    }
  }
  static ResourceKey<Recipe<?>> key(String name){return ResourceKey.create(Registries.RECIPE,Identifier.withDefaultNamespace(name));}
  static String item(ItemStack value){return InventoryVectors.item(value);}
  static AdvancementHolder advancement(RegistryAccess registries,String name,String trigger,String conditions,String reward){
    var json="{\"criteria\":{\"a\":{\"trigger\":\""+trigger+"\",\"conditions\":"+conditions+"}},\"rewards\":{\"recipes\":[\""+reward+"\"]}}";
    return new AdvancementHolder(Identifier.parse("test:"+name),Advancement.CODEC.parse(registries.createSerializationContext(JsonOps.INSTANCE),JsonParser.parseString(json)).getOrThrow());
  }
  static ContainerListener playerListener(ServerPlayer player)throws Exception {
    for(int i=1;i<30;i++)try{
      var type=Class.forName(ServerPlayer.class.getName()+"$"+i);
      if(ContainerListener.class.isAssignableFrom(type)){
        var constructor=type.getDeclaredConstructor(ServerPlayer.class);constructor.setAccessible(true);
        return (ContainerListener)constructor.newInstance(player);
      }
    }catch(ClassNotFoundException ignored){}
    throw new AssertionError("native ServerPlayer container listener missing");
  }
  static void scenario(String name,RecipeManager recipes,RegistryAccess registries)throws Exception {
    boolean furnace=name.startsWith("furnace"),quick=name.endsWith("quick");
    var events=new ArrayList<String>();
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);
    var world=(World)((sun.misc.Unsafe)field.get(null)).allocateInstance(World.class);
    ClearGridVectors.copyFields(new InventoryVectors.MenuLevel(registries),world,Level.class);
    world.recipes=recipes;world.server=mock(MinecraftServer.class);world.rules=new GameRules(net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);
    world.trace=events;world.orbs=new ArrayList<>();world.nextId=1;when(world.server.getRecipeManager()).thenReturn(recipes);
    when(world.server.reloadableRegistries()).thenReturn(new ReloadableServerRegistries.Holder(registries));
    var player=mock(ServerPlayer.class);var inventory=new Inventory(player,new EntityEquipment());
    var serverField=ServerPlayer.class.getDeclaredField("server");serverField.setAccessible(true);serverField.set(player,world.server);
    var book=new ServerRecipeBook((id,output)->recipes.listDisplaysForRecipe(id,output));
    when(player.level()).thenReturn(world);when(player.getInventory()).thenReturn(inventory);when(player.getAbilities()).thenReturn(new Abilities());
    when(player.position()).thenReturn(new Vec3(12.5,64,8.5));when(player.getRecipeBook()).thenReturn(book);when(player.gameMode()).thenReturn(GameType.SURVIVAL);
    doCallRealMethod().when(player).triggerRecipeCrafted(any(),anyList());
    doCallRealMethod().when(player).awardRecipesByKey(anyList());
    when(player.awardRecipes(anyCollection())).thenAnswer(call->book.addRecipes(call.getArgument(0),player));
    doCallRealMethod().when(player).getEquipmentSlotForItem(any());
    var stats=new HashMap<Stat<?>,Integer>();doAnswer(call->{Stat<?> stat=call.getArgument(0);stats.merge(stat,call.getArgument(1),Integer::sum);return null;}).when(player).awardStat(any(Stat.class),anyInt());
    var names=new HashMap<Integer,String>();for(String id:KEYS)recipes.listDisplaysForRecipe(key(id),display->names.put(display.id().index(),id));
    player.connection=mock(ServerGamePacketListenerImpl.class);
    doAnswer(call->{
      Packet<?> packet=call.getArgument(0);
      if(packet instanceof ClientboundRecipeBookAddPacket p)events.add("B:"+String.join(",",p.entries().stream().map(e->names.get(e.contents().id().index())).toList()));
      if(packet instanceof ClientboundContainerSetSlotPacket p)events.add("S:"+p.getSlot()+":"+item(p.getItem()));
      return null;
    }).when(player.connection).send(any(Packet.class));
    var holders=new ArrayList<AdvancementHolder>();
    if(name.equals("inventory")){
      holders.add(advancement(registries,"stone","inventory_changed","{\"items\":[{\"items\":\"stone\",\"count\":4}]}","oak_planks"));
      holders.add(advancement(registries,"dirt","inventory_changed","{\"items\":[{\"items\":\"dirt\"}]}","crafting_table"));
      holders.add(advancement(registries,"iron","inventory_changed","{\"items\":[{\"items\":\"iron_ingot\"}]}","furnace"));
    }else{
      String used=furnace?"iron_ingot_from_smelting_raw_iron":"oak_planks";
      holders.add(advancement(registries,"crafted","recipe_crafted","{\"recipes\":\""+used+"\"}","stick"));
      // XP consumes the first four RNG operations. Moving this predicate
      // before XP would award a different recipe, making that order observable.
      String chance=furnace?",\"player\":{\"type\":\"random_chance\",\"chance\":0.5}":"";
      holders.add(advancement(registries,"unlocked","recipe_unlocked","{\"recipes\":\""+used+"\""+chance+"}","crafting_table"));
    }
    var tree=new AdvancementTree();tree.addAll(holders);var manager=mock(ServerAdvancementManager.class);
    when(manager.getAllAdvancements()).thenReturn(holders);when(manager.tree()).thenReturn(tree);
    when(manager.get(any())).thenAnswer(call->holders.stream().filter(h->h.id().equals(call.getArgument(0))).findFirst().orElse(null));
    var folder=Files.createTempDirectory("ferraloom-menu-producers");
    var progress=new PlayerAdvancements(mock(com.mojang.datafixers.DataFixer.class),mock(net.minecraft.server.players.PlayerList.class),manager,folder.resolve("absent.json"),player);
    when(player.getAdvancements()).thenReturn(progress);
    AbstractContainerMenu menu;FurnaceBlockEntity entity=null;
    if(furnace){
      entity=new FurnaceBlockEntity(POS,Blocks.FURNACE.defaultBlockState());entity.setLevel(world);
      entity.setItem(2,new ItemStack(Items.IRON_INGOT,3));entity.setRecipeUsed(recipes.byKey(key("iron_ingot_from_smelting_raw_iron")).orElseThrow());
      for(int i=1;i<7;i++)entity.setRecipeUsed(recipes.byKey(key("iron_ingot_from_smelting_raw_iron")).orElseThrow());
      menu=new FurnaceMenu(7,inventory,entity,new SimpleContainerData(4));
    }else{
      menu=new InventoryMenu(inventory,true,player);
      if(name.equals("inventory")){
        var ignored=new ResultSlot(player,new TransientCraftingContainer(menu,2,2),inventory,1,0,0);ignored.index=37;menu.slots.set(37,ignored);
        var included=new FurnaceResultSlot(player,inventory,2,0,0);included.index=38;menu.slots.set(38,included);
      }else menu.getSlot(1).set(new ItemStack(Items.OAK_LOG));
    }
    if(name.equals("inventory"))menu.addSlotListener(new ContainerListener(){
      public void slotChanged(AbstractContainerMenu receiver,int index,ItemStack copy){
        if(index==36 && copy.is(Items.STONE))copy.grow(3);
      }
      public void dataChanged(AbstractContainerMenu receiver,int index,int value){}
    });
    menu.addSlotListener(playerListener(player));
    var hash=new InventoryVectors.Sync(registries);
    menu.setSynchronizer(new ContainerSynchronizer(){
      public RemoteSlot createSlot(){return hash.createSlot();}
      public void sendInitialData(AbstractContainerMenu m,List<ItemStack> values,ItemStack cursor,int[] data){}
      public void sendSlotChange(AbstractContainerMenu m,int slot,ItemStack value){events.add("S:"+slot+":"+item(value));}
      public void sendCarriedChange(AbstractContainerMenu m,ItemStack value){events.add("C:"+item(value));}
      public void sendDataChange(AbstractContainerMenu m,int slot,int value){}
    });
    events.clear();stats.clear();world.getRandom().setSeed(263777);
    if(name.equals("inventory")){
      inventory.setItem(0,new ItemStack(Items.STONE));inventory.setItem(1,new ItemStack(Items.DIRT));inventory.setItem(2,new ItemStack(Items.IRON_INGOT));
    }else menu.clicked(furnace?2:0,0,quick?ContainerInput.QUICK_MOVE:ContainerInput.PICKUP,player);
    menu.broadcastChanges();
    var known=KEYS.stream().filter(id->book.contains(key(id))).toList();
    int crafted=stats.getOrDefault(Stats.ITEM_CRAFTED.get(furnace?Items.IRON_INGOT:Items.OAK_PLANKS),0);
    var count=ExperienceOrb.class.getDeclaredField("count");count.setAccessible(true);
    var orbs=new ArrayList<String>();for(var orb:world.orbs)orbs.add(orb.getValue()+":"+count.getInt(orb));
    System.out.println("producer\t"+name+"\t"+String.join(";",events)+"\t"+InventoryVectors.slots(menu)+"\t"+item(menu.getCarried())+"\t"+String.join(",",known)+"\t"+crafted+"\t"+String.join(",",orbs)+"\t"+world.getRandom().nextInt());
    progress.clearTriggers();Files.delete(folder);
  }
  static void run(RecipeManager recipes,RegistryAccess registries)throws Exception {
    for(String name:List.of("inventory","crafting-pickup","crafting-quick","furnace-pickup","furnace-quick"))scenario(name,recipes,registries);
  }
}
