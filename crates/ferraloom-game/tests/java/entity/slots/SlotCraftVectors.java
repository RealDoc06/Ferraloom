import java.util.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.stats.ServerRecipeBook;
import net.minecraft.server.level.*;
import net.minecraft.server.network.*;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.*;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.network.protocol.game.ClientboundContainerSetSlotPacket;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real ServerPlayer#getSlot's craft setter, including both native callbacks. */
public final class SlotCraftVectors {
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
    var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
    try(var resources=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack))){
      var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);TagLoader.loadTagsForExistingRegistries(resources,builtins).forEach(Registry.PendingTags::apply);
      var world=RegistryDataLoader.load(resources,builtins.listRegistries().toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
      var context=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),world.listRegistries()));BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(p->p.apply());
      var recipes=RegistryDataLoader.load(resources,context.listRegistries().toList(),List.of(RegistryDataLoader.RELOADABLE_REGISTRIES.stream().filter(r->r.key()==Registries.RECIPE).findFirst().orElseThrow()),Runnable::run).join();
      var registries=new RegistryAccess.ImmutableRegistryAccess(Stream.of(builtins,world,recipes).flatMap(RegistryAccess::registries)).freeze();
      var manager=new RecipeManager(registries);manager.finalizeRecipeLoading(FeatureFlags.DEFAULT_FLAGS);
      for(boolean limited:List.of(false,true))for(boolean known:List.of(false,true)){
        var level=mock(ServerLevel.class);var server=mock(MinecraftServer.class);var rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);rules.set(GameRules.LIMITED_CRAFTING,limited,null);
        when(level.getServer()).thenReturn(server);when(server.getRecipeManager()).thenReturn(manager);when(level.recipeAccess()).thenReturn(manager);when(level.getGameRules()).thenReturn(rules);when(level.enabledFeatures()).thenReturn(FeatureFlags.DEFAULT_FLAGS);
        var player=mock(ServerPlayer.class);var inv=new Inventory(player,new EntityEquipment());when(player.getInventory()).thenReturn(inv);when(player.level()).thenReturn(level);when(player.gameMode()).thenReturn(GameType.SURVIVAL);when(player.getAbilities()).thenReturn(new Abilities());
        var book=mock(ServerRecipeBook.class);when(book.contains(any())).thenReturn(known);when(player.getRecipeBook()).thenReturn(book);
        var menu=new InventoryMenu(inv,true,player);
        var inventoryField=Player.class.getDeclaredField("inventory");inventoryField.setAccessible(true);inventoryField.set(player,inv);
        var menuField=Player.class.getDeclaredField("inventoryMenu");menuField.setAccessible(true);menuField.set(player,menu);
        player.containerMenu=ChestMenu.threeRows(7,inv);
        doCallRealMethod().when(player).getSlot(anyInt());
        var packets=new ArrayList<String>();var connection=mock(ServerGamePacketListenerImpl.class);player.connection=connection;
        doAnswer(call->{if(call.getArgument(0) instanceof ClientboundContainerSetSlotPacket p)packets.add(p.getContainerId()+":"+p.getStateId()+":"+p.getSlot()+":"+SlotVectors.describe(p.getItem()));else throw new AssertionError(call.getArgument(0));return null;}).when(connection).send(any());
        int step=0;
        for(var input:List.of(new ItemStack(Items.OAK_LOG),new ItemStack(Items.IRON_INGOT),ItemStack.EMPTY,ItemStack.EMPTY)){
          int slot=step==0||step==3?500:501;
          packets.clear();boolean result=player.getSlot(slot).set(input);
          System.out.println("slot-craft\t"+limited+"\t"+known+"\t"+step+++"\t"+slot+"\t"+SlotVectors.describe(input)+"\t"+result+"\t"+SlotVectors.describe(menu.getSlot(0).getItem())+"\t"+String.join(";",packets));
        }
      }
    }
  }
}
