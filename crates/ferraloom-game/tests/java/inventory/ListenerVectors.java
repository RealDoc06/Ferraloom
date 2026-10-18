import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;

/** Real broadcastChanges callbacks replace the slot or mutate its held receiver. */
public final class ListenerVectors {
  static void scenario(RegistryAccess registries, boolean replace) {
    var player = new InventoryVectors.MenuPlayer(new InventoryVectors.MenuLevel(registries), false);
    var menu = InventoryVectors.menu("inventory", player);
    var sync = new InventoryVectors.Sync(registries);
    var events = new ArrayList<String>();
    var nested = new boolean[]{false};
    menu.addSlotListener(new ContainerListener() {
      public void slotChanged(AbstractContainerMenu receiver, int index, ItemStack changed) {
        if (index != 36) return;
        events.add("A:" + InventoryVectors.item(changed) + ":" + (changed == receiver.getSlot(index).getItem()));
        if (!nested[0]) {
          nested[0] = true;
          if (replace) player.getInventory().setItem(0, new ItemStack(Items.DIRT, 2));
          else receiver.getSlot(index).getItem().grow(1);
          receiver.broadcastChanges();
          changed.grow(3);
          events.add("held:" + InventoryVectors.item(changed));
        }
      }
      public void dataChanged(AbstractContainerMenu receiver, int index, int value) {}
    });
    menu.addSlotListener(new ContainerListener() {
      public void slotChanged(AbstractContainerMenu receiver, int index, ItemStack changed) {
        if (index == 36) events.add("B:" + InventoryVectors.item(changed));
      }
      public void dataChanged(AbstractContainerMenu receiver, int index, int value) {}
    });
    menu.setSynchronizer(sync);
    sync.events.clear();
    player.getInventory().setItem(0, new ItemStack(Items.STONE, 5));
    menu.broadcastChanges();
    menu.broadcastChanges();
    System.out.println("listener\t" + (replace ? "replace" : "mutate") + "\t"
      + String.join(";", events) + "\t" + String.join(";", sync.events) + "\t"
      + InventoryVectors.item(menu.getSlot(36).getItem()));
  }
  public static void main(String[] args) throws Exception {
    var output = System.out;
    var errors = System.err;
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    System.setOut(output);
    System.setErr(errors);
    var registries = NeighborVectors.load();
    var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(), registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    scenario(registries, true);
    scenario(registries, false);
  }
}
