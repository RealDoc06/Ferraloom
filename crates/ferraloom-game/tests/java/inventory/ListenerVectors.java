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
  static ItemStack last(AbstractContainerMenu menu, int slot) throws Exception {
    var field = AbstractContainerMenu.class.getDeclaredField("lastSlots");
    field.setAccessible(true);
    return (ItemStack)((List<?>)field.get(menu)).get(slot);
  }
  static void alias(RegistryAccess registries, boolean replaceSource) throws Exception {
    var player = new InventoryVectors.MenuPlayer(new InventoryVectors.MenuLevel(registries), false);
    var source = InventoryVectors.menu("inventory", player);
    var target = InventoryVectors.menu("inventory", player);
    player.getInventory().setItem(0, new ItemStack(Items.STONE, 5));
    source.broadcastChanges();
    var held = last(source, 36);
    target.transferState(source);
    var events = new ArrayList<String>();
    events.add("alias:" + (held == last(target, 36)));
    held.grow(3);
    events.add("shared:" + InventoryVectors.item(last(source, 36)) + ":" + InventoryVectors.item(last(target, 36)));
    if (replaceSource) {
      player.getInventory().setItem(0, new ItemStack(Items.DIRT, 2));
      source.broadcastChanges();
    } else target.broadcastChanges();
    events.add("owners:" + (held == last(source, 36)) + ":" + (held == last(target, 36)));
    held.grow(1);
    events.add("held:" + InventoryVectors.item(held));
    events.add("final:" + InventoryVectors.item(last(source, 36)) + ":" + InventoryVectors.item(last(target, 36)));
    System.out.println("listener\talias-" + (replaceSource ? "source" : "target") + "\t" + String.join(";", events));
  }
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
  static void operationStack(boolean copy) {
    var container = new net.minecraft.world.SimpleContainer(2);
    container.setItem(0, new ItemStack(Items.STONE, 10));
    var source = container.getItem(0);
    var fresh = copy ? source.copyWithCount(4) : source.split(4);
    var events = new ArrayList<String>();
    events.add("source:" + InventoryVectors.item(source));
    container.setItem(1, fresh);
    events.add("alias:" + (fresh == container.getItem(1)));
    container.setItem(1, new ItemStack(Items.DIRT, 3));
    fresh.grow(2);
    events.add("destination:" + InventoryVectors.item(container.getItem(1)));
    events.add("held:" + InventoryVectors.item(fresh));
    System.out.println("listener\toperation-" + (copy ? "copy" : "split") + "\t" + String.join(";", events));
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
    alias(registries, true);
    alias(registries, false);
    operationStack(false);
    operationStack(true);
  }
}
