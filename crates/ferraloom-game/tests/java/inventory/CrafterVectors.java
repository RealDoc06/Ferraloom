import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.*;
import net.minecraft.server.packs.resources.*;
import java.util.stream.Stream;
import net.minecraft.server.level.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.*;

/** Real CrafterMenu class listener and CrafterSlot setter on a ServerLevel. */
public final class CrafterVectors {
  static final class ServerFixture extends ServerLevel {
    RecipeManager recipes;
    int nextId;
    @Override public int getNextEntityId(){return ++nextId;}
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public RecipeManager recipeAccess(){return recipes;}
  }
  static void copyFields(Object from,Object to,Class<?> first)throws Exception {
    for(Class<?> owner=first;owner!=Object.class;owner=owner.getSuperclass())for(Field field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){
      field.setAccessible(true);field.set(to,field.get(from));
    }
  }
  public static void run(RegistryAccess registries)throws Exception {
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);
    copyFields(new InventoryVectors.MenuLevel(registries),server,Level.class);
    var location=new PackLocationInfo("vanilla",net.minecraft.network.chat.Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
    var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
    try(var resources=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack))){
      var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
      var context=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),registries.listRegistries()));
      var recipes=RegistryDataLoader.load(resources,context.listRegistries().toList(),List.of(RegistryDataLoader.RELOADABLE_REGISTRIES.stream().filter(r->r.key()==Registries.RECIPE).findFirst().orElseThrow()),Runnable::run).join();
      server.recipes=new RecipeManager(recipes);
    }
    var player=(ServerPlayer)allocator.allocateInstance(ServerPlayer.class);
    var template=new InventoryVectors.MenuPlayer(server,false);
    copyFields(template,player,net.minecraft.world.entity.player.Player.class);
    var inventoryField=net.minecraft.world.entity.player.Player.class.getDeclaredField("inventory");inventoryField.setAccessible(true);
    var equipmentField=net.minecraft.world.entity.player.Inventory.class.getDeclaredField("equipment");equipmentField.setAccessible(true);
    inventoryField.set(player,new net.minecraft.world.entity.player.Inventory(player,(net.minecraft.world.entity.EntityEquipment)equipmentField.get(template.getInventory())));
    var grid=new net.minecraft.world.level.block.entity.CrafterBlockEntity(BlockPos.ZERO,net.minecraft.world.level.block.Blocks.CRAFTER.defaultBlockState());
    var menu=new CrafterMenu(7,player.getInventory(),grid,new SimpleContainerData(10));
    var events=new ArrayList<String>();
    menu.addSlotListener(new ContainerListener(){
      public void slotChanged(AbstractContainerMenu receiver,int slot,ItemStack stack){
        events.add(slot+":"+InventoryVectors.item(stack)+":"+InventoryVectors.item(receiver.getSlot(45).getItem()));
      }
      public void dataChanged(AbstractContainerMenu receiver,int slot,int value){}
    });
    grid.setItem(0,new ItemStack(Items.OAK_LOG));
    menu.broadcastChanges();
    var held=menu.getSlot(45).getItem();
    events.add("set:"+InventoryVectors.item(held));
    player.getInventory().setItem(0,new ItemStack(Items.DIRT));
    menu.broadcastChanges();
    events.add("refresh:"+(held==menu.getSlot(45).getItem())+":"+InventoryVectors.item(held));
    System.out.println("listener\tcrafter-internal\t"+String.join(";",events));
    menu.getSlot(0).set(new ItemStack(Items.DIRT));
    System.out.println("listener\tcrafter-slot\tempty:"+InventoryVectors.item(menu.getSlot(45).getItem()));
  }
}
