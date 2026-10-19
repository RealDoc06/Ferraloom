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
  static final class InsertionContainer extends net.minecraft.world.SimpleContainer {
    final net.minecraft.world.SimpleContainer source = new net.minecraft.world.SimpleContainer(1);
    final ArrayList<String> events = new ArrayList<>();
    final boolean replace;
    ItemStack input, oldTarget;
    boolean armed, fired;
    InsertionContainer(boolean replace) {
      super(1);
      this.replace = replace;
      source.setItem(0, new ItemStack(Items.STONE, 10));
      setItem(0, new ItemStack(Items.STONE, 60));
      input = source.getItem(0);
      oldTarget = getItem(0);
      armed = true;
    }
    @Override public void setChanged() {
      super.setChanged();
      if (!armed) return;
      events.add("changed:" + InventoryVectors.item(input) + ":" + InventoryVectors.item(source.getItem(0))
        + ":" + InventoryVectors.item(getItem(0)) + ":" + (getItem(0) == oldTarget));
      if (!fired) {
        fired = true;
        if (replace) {
          source.setItem(0, new ItemStack(Items.DIAMOND, 7));
          setItem(0, new ItemStack(Items.DIRT, 2));
        }
        input.grow(1);
      }
    }
  }
  static void safeInsert(boolean replace) {
    var container = new InsertionContainer(replace);
    var returned = new Slot(container, 0, 0, 0).safeInsert(container.input, 10);
    container.events.add("returned:" + (returned == container.input) + ":" + InventoryVectors.item(returned));
    container.events.add("heldTarget:" + InventoryVectors.item(container.oldTarget));
    System.out.println("listener\tinsert-" + (replace ? "replace" : "mutate") + "\t" + String.join(";", container.events));
  }
  static void pickup(RegistryAccess registries) {
    var player = new InventoryVectors.MenuPlayer(new InventoryVectors.MenuLevel(registries), false);
    var menu = InventoryVectors.menu("chest1", player);
    menu.getSlot(0).setByPlayer(new ItemStack(Items.STONE, 60));
    menu.setCarried(new ItemStack(Items.STONE, 10));
    var carried = menu.getCarried();
    var target = menu.getSlot(0).getItem();
    var events = new ArrayList<String>();
    menu.clicked(0, 0, ContainerInput.PICKUP, player);
    events.add("merge:" + (carried == menu.getCarried()) + ":" + (target == menu.getSlot(0).getItem())
      + ":" + InventoryVectors.item(carried) + ":" + InventoryVectors.item(target));
    menu.getSlot(0).setByPlayer(new ItemStack(Items.DIRT, 2));
    menu.clicked(0, 0, ContainerInput.PICKUP, player);
    events.add("swap:" + (carried == menu.getSlot(0).getItem()) + ":" + InventoryVectors.item(menu.getCarried())
      + ":" + InventoryVectors.item(carried) + ":" + InventoryVectors.item(target));
    carried.grow(1);
    events.add("held:" + InventoryVectors.item(menu.getSlot(0).getItem()));
    var patched = new ItemStack(Items.STONE, 3);
    patched.set(DataComponents.BUNDLE_CONTENTS, net.minecraft.world.item.component.BundleContents.EMPTY);
    menu.getSlot(1).setByPlayer(patched);
    menu.setCarried(ItemStack.EMPTY);
    menu.clicked(1, 0, ContainerInput.PICKUP, player);
    events.add("component:" + menu.getCarried().has(DataComponents.BUNDLE_CONTENTS) + ":" + InventoryVectors.item(menu.getCarried()));
    System.out.println("listener\tpickup\t" + String.join(";", events));
  }
  static void inventoryAdd(RegistryAccess registries, String kind) {
    var player = new InventoryVectors.MenuPlayer(new InventoryVectors.MenuLevel(registries), false);
    var inventory = player.getInventory();
    for (int slot = 0; slot < 36; slot++) inventory.setItem(slot, new ItemStack(Items.DIRT, 64));
    inventory.setItem(0, new ItemStack(Items.STONE, 60));
    inventory.setItem(40, new ItemStack(Items.STONE, 63));
    if (kind.equals("empty") || kind.equals("damaged")) inventory.setItem(1, ItemStack.EMPTY);
    player.getAbilities().instabuild = kind.equals("infinite");
    var source = new net.minecraft.world.SimpleContainer(1);
    var input = new ItemStack(kind.equals("damaged") ? Items.IRON_PICKAXE : Items.STONE, kind.equals("damaged") ? 1 : 10);
    if (kind.equals("damaged")) input.set(DataComponents.DAMAGE, 7);
    source.setItem(0, input);
    if (kind.equals("detached")) source.setItem(0, new ItemStack(Items.DIRT, 7));
    var events = new ArrayList<String>();
    events.add("added:" + inventory.add(input));
    events.add("left:" + InventoryVectors.item(input));
    for (int slot : new int[]{0, 1, 40}) events.add(slot + ":" + InventoryVectors.item(inventory.getItem(slot)));
    input.grow(2);
    events.add("held:" + InventoryVectors.item(input));
    events.add("owner:" + InventoryVectors.item(source.getItem(0)));
    System.out.println("listener\tinventory-add-" + kind + "\t" + String.join(";", events));
  }
  static void swap(RegistryAccess registries) {
    var player = new InventoryVectors.MenuPlayer(new InventoryVectors.MenuLevel(registries), false);
    var menu = InventoryVectors.menu("chest1", player);
    var inventory = player.getInventory();
    inventory.setItem(0, new ItemStack(Items.STONE, 70));
    var held = inventory.getItem(0);
    menu.clicked(36, 0, ContainerInput.SWAP, player);
    var events = new ArrayList<String>();
    events.add("self:" + InventoryVectors.item(held) + ":" + InventoryVectors.item(inventory.getItem(0)) + ":" + InventoryVectors.item(inventory.getItem(1)));
    held.grow(2);
    events.add("detached:" + InventoryVectors.item(held) + ":" + InventoryVectors.item(inventory.getItem(0)));
    inventory.setItem(0, new ItemStack(Items.STONE, 10));
    menu.getSlot(0).setByPlayer(new ItemStack(Items.DIRT, 2));
    var source = inventory.getItem(0);
    var target = menu.getSlot(0).getItem();
    menu.clicked(0, 0, ContainerInput.SWAP, player);
    events.add("alias:" + (source == menu.getSlot(0).getItem()) + ":" + (target == inventory.getItem(0)));
    source.grow(1);
    target.grow(1);
    events.add("current:" + InventoryVectors.item(menu.getSlot(0).getItem()) + ":" + InventoryVectors.item(inventory.getItem(0)));
    System.out.println("listener\tswap\t" + String.join(";", events));
  }
  static void quickcraft(RegistryAccess registries) {
    var player = new InventoryVectors.MenuPlayer(new InventoryVectors.MenuLevel(registries), false);
    var menu = InventoryVectors.menu("chest1", player);
    menu.setCarried(new ItemStack(Items.STONE, 10));
    var held = menu.getCarried();
    menu.clicked(-999, 0, ContainerInput.QUICK_CRAFT, player);
    menu.clicked(0, 1, ContainerInput.QUICK_CRAFT, player);
    menu.clicked(1, 1, ContainerInput.QUICK_CRAFT, player);
    menu.clicked(-999, 2, ContainerInput.QUICK_CRAFT, player);
    var current = menu.getCarried();
    var events = new ArrayList<String>();
    events.add("copy:" + (held == current) + ":" + InventoryVectors.item(held) + ":" + InventoryVectors.item(current));
    held.grow(2);
    current.grow(2);
    events.add("held:" + InventoryVectors.item(held) + ":" + InventoryVectors.item(menu.getCarried()));
    events.add("targets:" + InventoryVectors.item(menu.getSlot(0).getItem()) + ":" + InventoryVectors.item(menu.getSlot(1).getItem()));
    System.out.println("listener\tquickcraft\t" + String.join(";", events));
  }
  static void resultAchievements(RegistryAccess registries, int amount) throws Exception {
    var events = new ArrayList<String>();
    var grid = new TransientCraftingContainer[1];
    var slot = new ResultSlot[1];
    var removeCount = ResultSlot.class.getDeclaredField("removeCount");
    removeCount.setAccessible(true);
    var player = new net.minecraft.world.entity.player.Player(new InventoryVectors.MenuLevel(registries),
      new com.mojang.authlib.GameProfile(new UUID(3, 4), "result")) {
      @Override public net.minecraft.world.level.GameType gameMode() { return net.minecraft.world.level.GameType.SURVIVAL; }
      @Override public void awardStat(net.minecraft.stats.Stat<?> stat, int count) {
        events.add("crafted:" + count);
        grid[0].setItem(0, new ItemStack(Items.DIRT, 3));
        try { removeCount.setInt(slot[0], 19); } catch (Exception error) { throw new RuntimeException(error); }
      }
    };
    var menu = ChestMenu.oneRow(7, player.getInventory());
    grid[0] = new TransientCraftingContainer(menu, 2, 2);
    grid[0].setItem(0, new ItemStack(Items.STONE, 5));
    var result = new ResultContainer() {
      @Override public void awardUsedRecipes(net.minecraft.world.entity.player.Player receiver, List<ItemStack> input) {
        try { events.add("award:" + removeCount.getInt(slot[0]) + ":" + input.size() + ":" + InventoryVectors.item(input.get(0))); }
        catch (Exception error) { throw new RuntimeException(error); }
        grid[0].setItem(0, new ItemStack(Items.DIAMOND, 7));
        events.add("captured:" + InventoryVectors.item(input.get(0)));
      }
    };
    class ExposedResultSlot extends ResultSlot {
      ExposedResultSlot() { super(player, grid[0], result, 0, 0, 0); }
      void check(ItemStack stack) { checkTakeAchievements(stack); }
    }
    var exposed = new ExposedResultSlot();
    slot[0] = exposed;
    removeCount.setInt(exposed, amount);
    exposed.check(new ItemStack(Items.STONE, 4));
    events.add("after:" + removeCount.getInt(exposed) + ":" + InventoryVectors.item(grid[0].getItem(0)));
    System.out.println("listener\tresult-achievements-" + amount + "\t" + String.join(";", events));
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
    safeInsert(false);
    safeInsert(true);
    pickup(registries);
    for (var kind : new String[]{"partial", "infinite", "empty", "damaged", "detached"}) inventoryAdd(registries, kind);
    swap(registries);
    quickcraft(registries);
    resultAchievements(registries, 0);
    resultAchievements(registries, 4);
    CrafterVectors.run(registries);
  }
}
