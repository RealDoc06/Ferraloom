import java.util.*;
import java.util.stream.Stream;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.core.component.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.*;
import net.minecraft.stats.*;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.flag.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.level.block.*;
import net.minecraft.network.protocol.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.*;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.*;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.*;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real server-side menus/recipe calculation and ResultSlot, with only world
 * services, advancement listeners and network delivery supplied by the fixture. */
public final class CraftingMenuVectors {
  static RecipeManager manager;
  static RegistryAccess.Frozen registries;
  static String item(ItemStack s){return InventoryVectors.item(s);}
  static String events(List<String> e){return e.isEmpty()?"-":String.join(";",e);}
  static void scenario(String name,boolean table,List<int[]> clicks) throws Exception {
    var level=mock(ServerLevel.class);var server=mock(MinecraftServer.class);
    when(level.getServer()).thenReturn(server);when(server.getRecipeManager()).thenReturn(manager);when(level.recipeAccess()).thenReturn(manager);
    when(level.enabledFeatures()).thenReturn(FeatureFlags.DEFAULT_FLAGS);when(level.getGameRules()).thenReturn(new GameRules(FeatureFlags.DEFAULT_FLAGS));
    var player=mock(ServerPlayer.class);when(player.getAdvancements()).thenReturn(mock(PlayerAdvancements.class));var abilities=new Abilities();var inv=new Inventory(player,new EntityEquipment());
    when(player.level()).thenReturn(level);when(player.getInventory()).thenReturn(inv);when(player.getAbilities()).thenReturn(abilities);when(player.gameMode()).thenReturn(GameType.SURVIVAL);
    var book=new ServerRecipeBook((id,output)->manager.listDisplaysForRecipe(id,output));when(player.getRecipeBook()).thenReturn(book);
    doCallRealMethod().when(player).getEquipmentSlotForItem(any());
    var connection=mock(ServerGamePacketListenerImpl.class);player.connection=connection;
    AbstractContainerMenu menu=table?new CraftingMenu(7,inv,ContainerLevelAccess.create(level,net.minecraft.core.BlockPos.ZERO)):new InventoryMenu(inv,true,player);
    var sync=new InventoryVectors.Sync(registries);var stats=new ArrayList<String>();var awards=new ArrayList<String>();
    doAnswer(call->{var packet=call.getArgument(0);if(packet instanceof ClientboundContainerSetSlotPacket p)sync.events.add("S:"+p.getStateId()+":"+p.getSlot()+":"+item(p.getItem()));else if(packet instanceof ClientboundRecipeBookAddPacket p)sync.events.add("B:"+p.entries().size());else throw new AssertionError(packet);return null;}).when(connection).send(any(Packet.class));
    doAnswer(call->{Stat<?> stat=call.getArgument(0);stats.add(BuiltInRegistries.ITEM.getId((Item)stat.getValue())+":"+call.getArgument(1));return null;}).when(player).awardStat(any(Stat.class),anyInt());
    doAnswer(call->{RecipeHolder<?> recipe=call.getArgument(0);awards.add(recipe.id().identifier().toString());return null;}).when(player).triggerRecipeCrafted(any(),anyList());
    when(player.awardRecipes(anyCollection())).thenAnswer(call->book.addRecipes(call.getArgument(0),player));
    menu.setSynchronizer(sync);sync.events.clear();
    if(name.startsWith("fallback")) {
      menu.getSlot(1).set(new ItemStack(Items.STONE,name.endsWith("one")?1:3));
      menu.getSlot(0).set(new ItemStack(Items.STONE,4));
    } else if(name.equals("cake")) {
      for(int i=1;i<=3;i++)menu.getSlot(i).set(new ItemStack(Items.MILK_BUCKET));
      menu.getSlot(4).set(new ItemStack(Items.SUGAR,2));menu.getSlot(5).set(new ItemStack(Items.EGG,2));menu.getSlot(6).set(new ItemStack(Items.SUGAR,2));
      for(int i=7;i<=9;i++)menu.getSlot(i).set(new ItemStack(Items.WHEAT,2));
    } else {
      menu.getSlot(table?37:36).set(new ItemStack(Items.OAK_LOG,8));
      menu.getSlot((table?37:36)+1).set(new ItemStack(Items.OAK_PLANKS,12));
    }
    menu.broadcastFullState();sync.events.clear();stats.clear();awards.clear();
    System.out.println("start\t"+name+"\t"+(table?"crafting":"inventory")+"\t"+InventoryVectors.slots(menu)+"\t"+menu.getStateId());
    int step=0;
    for(var c:clicks){
      menu.suppressRemoteUpdates();menu.clicked(c[0],c[1],ContainerInput.values()[c[2]],player);menu.resumeRemoteUpdates();menu.broadcastChanges();
      System.out.println("click\t"+name+"\t"+step+++"\t"+c[0]+"\t"+c[1]+"\t"+c[2]+"\t"+InventoryVectors.slots(menu)+"\t"+item(menu.getCarried())+"\t"+menu.getStateId()+"\t"+events(sync.events)+"\t"+events(stats)+"\t"+events(awards));sync.events.clear();stats.clear();awards.clear();
    }
    for(String recipeName:List.of("oak_planks","stick","crafting_table","furnace"))for(boolean maximum:new boolean[]{false,true}) {
      var holder=manager.byKey(ResourceKey.create(Registries.RECIPE,Identifier.withDefaultNamespace(recipeName))).orElseThrow();
      var action=((AbstractCraftingMenu)menu).handlePlacement(maximum,false,holder,level,inv);menu.broadcastChanges();
      System.out.println("place\t"+name+"\t"+step+++"\t"+recipeName+"\t"+maximum+"\t"+action+"\t"+InventoryVectors.slots(menu)+"\t"+item(menu.getCarried())+"\t"+menu.getStateId()+"\t"+events(sync.events)+"\t"+events(stats)+"\t"+events(awards));sync.events.clear();stats.clear();awards.clear();
    }
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var location=new PackLocationInfo("vanilla",net.minecraft.network.chat.Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
    var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
    try(var resources=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack))){
      var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);TagLoader.loadTagsForExistingRegistries(resources,builtins).forEach(Registry.PendingTags::apply);
      var world=RegistryDataLoader.load(resources,builtins.listRegistries().toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
      var context=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),world.listRegistries()));BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(p->p.apply());
      var recipes=RegistryDataLoader.load(resources,context.listRegistries().toList(),List.of(RegistryDataLoader.RELOADABLE_REGISTRIES.stream().filter(r->r.key()==Registries.RECIPE).findFirst().orElseThrow()),Runnable::run).join();
      registries=new RegistryAccess.ImmutableRegistryAccess(Stream.of(builtins,world,recipes).flatMap(RegistryAccess::registries)).freeze();for(var item:BuiltInRegistries.ITEM)if(!item.isEnabled(FeatureFlags.DEFAULT_FLAGS))throw new AssertionError("disabled baseline item: "+item);manager=new RecipeManager(registries);manager.finalizeRecipeLoading(FeatureFlags.DEFAULT_FLAGS);
      for(boolean table:new boolean[]{false,true}) {
        int hotbar=table?37:36;
        var clicks=new ArrayList<int[]>();
        Collections.addAll(clicks,new int[]{hotbar,0,0},new int[]{1,1,0},new int[]{hotbar,0,0},new int[]{0,1,0},new int[]{hotbar+2,0,0},new int[]{0,0,1});
        Collections.addAll(clicks,new int[]{hotbar+1,0,0},new int[]{1,1,0},new int[]{table?4:3,1,0},new int[]{hotbar+1,0,0},new int[]{0,0,1});
        Collections.addAll(clicks,new int[]{hotbar,0,0},new int[]{-999,0,5},new int[]{1,1,5},new int[]{2,1,5},new int[]{-999,2,5},new int[]{0,40,2},new int[]{0,1,4});
        var random=new Random(263777);for(int i=0;i<100;i++)clicks.add(new int[]{random.nextInt(46),random.nextInt(2),i%4==0?1:0});
        scenario(table?"table":"player",table,clicks);
      }
      scenario("fallback-three",false,List.of(new int[]{0,1,0}));scenario("fallback-one",false,List.of(new int[]{0,0,0}));
      scenario("cake",true,List.of(new int[]{0,0,0}));
      ClearGridVectors.run(manager,registries);
    }
  }
}
