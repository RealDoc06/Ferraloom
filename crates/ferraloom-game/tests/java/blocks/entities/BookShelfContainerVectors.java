import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;

/** Actual ListBackedContainer queries and the single native Hopper transfer. */
public final class BookShelfContainerVectors {
  static final BlockPos POS = BookShelfHookVectors.POS;
  static final class World extends NeighborFixture {
    final Map<BlockPos, BlockEntity> receivers = new HashMap<>();
    ChiseledBookShelfBlockEntity shelf;
    final List<String> trace = new ArrayList<>();
    World(RegistryAccess access) { super(access, 100); }
    @Override public net.minecraft.world.level.block.state.BlockState getBlockState(BlockPos pos) { return blocks.getOrDefault(pos, Blocks.AIR.defaultBlockState()); }
    @Override public boolean setBlock(BlockPos pos, net.minecraft.world.level.block.state.BlockState state, int flags, int limit) {
      var old = blocks.put(pos.immutable(), state);
      if (old == state) return false;
      if (pos.equals(POS)) shelf.setBlockState(state);
      trace.add("u:" + Block.getId(state)); return true;
    }
    @Override public void blockEntityChanged(BlockPos pos) { trace.add("changed:" + pos.getY()); }
    @Override public void updateNeighbourForOutputSignal(BlockPos pos, Block block) { trace.add("analog:" + pos.getY()); }
    @Override public void gameEvent(Holder event, net.minecraft.world.phys.Vec3 position, net.minecraft.world.level.gameevent.GameEvent.Context context) {
      trace.add("g:" + BuiltInRegistries.GAME_EVENT.getId((net.minecraft.world.level.gameevent.GameEvent)event.value()) + ":0:" + (context.affectedState() == null ? -1 : Block.getId(context.affectedState())));
    }
    void prepare(int mask) {
      blocks.clear(); receivers.clear(); trace.clear();
      var state = Blocks.CHISELED_BOOKSHELF.defaultBlockState();
      shelf = new ChiseledBookShelfBlockEntity(POS, state);
    }
    @Override public BlockEntity getBlockEntity(BlockPos pos) { return receivers.get(pos); }
    void put(BlockEntity entity) {
      receivers.put(entity.getBlockPos(), entity);
      blocks.put(entity.getBlockPos(), entity.getBlockState());
      entity.setLevel(this);
    }
  }
  static BlockEntity container(World world, String name, BlockPos pos) {
    var state = BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(name)).defaultBlockState();
    var entity = ((EntityBlock)state.getBlock()).newBlockEntity(pos, state);
    world.put(entity);
    return entity;
  }
  public static void main(String[] args) throws Exception {
    var output = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
    var data = NeighborVectors.load(); var builtin = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var access = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtin.registries(), data.registries())).freeze();
    BookShelfHookVectors.context = access;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(access).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    var world = new World(access);
    for (int stored : new int[]{0, 1, 2, 99}) for (var item : new Item[]{Items.BOOK, Items.ENCHANTED_BOOK, Items.STONE, Items.AIR}) {
      world.prepare(0); world.put(world.shelf);
      for (int slot = 0; slot < 6; slot++) world.shelf.getItems().set(slot, new ItemStack(Items.BOOK, stored));
      var incoming = new ItemStack(item, 1);
      System.out.println("shelf-admit\t" + stored + "\t" + BuiltInRegistries.ITEM.getKey(item) + "\t" + world.shelf.getContainerSize() + "\t" + world.shelf.getMaxStackSize() + "\t" + world.shelf.isEmpty() + "\t" + world.shelf.canPlaceItem(0, incoming));
    }
    var take = HopperBlockEntity.class.getDeclaredMethod("tryTakeInItemFromSlot", Hopper.class, Container.class, int.class, Direction.class); take.setAccessible(true);
    for (int sourceCount : new int[]{1, 2, 4, 65, 99}) for (int destinationCount : new int[]{0, 60, 63, 64}) {
      world.prepare(0); world.receivers.clear(); world.put(world.shelf);
      var hopper = (HopperBlockEntity)container(world, "hopper", POS.below());
      for (int slot = 0; slot < hopper.getContainerSize(); slot++) hopper.setItem(slot, new ItemStack(Items.BOOK, destinationCount));
      var original = new ItemStack(Items.BOOK, sourceCount); world.shelf.getItems().set(0, original);
      boolean allowed = world.shelf.canTakeItem(hopper, 0, original);
      world.trace.clear(); boolean moved = (boolean)take.invoke(null, hopper, world.shelf, 0, Direction.DOWN);
      System.out.println("shelf-take\t" + sourceCount + "\t" + destinationCount + "\t" + allowed + "\t" + moved + "\t" + world.shelf.saveCustomOnly(access) + "\t" + hopper.saveCustomOnly(access) + "\t" + BookShelfHookVectors.stack(original) + "\t" + (hopper.getItem(0) == original) + "\t" + String.join(";", world.trace));
    }
    for (String name : List.of("chest", "hopper")) for (int stored : new int[]{0, 60, 64}) for (var item : new Item[]{Items.STONE, Items.BOOK}) {
      world.prepare(0); world.receivers.clear(); var target = container(world, name, POS.below()); var inventory = (Container)target;
      for (int slot = 0; slot < inventory.getContainerSize(); slot++) {
        inventory.setItem(slot, new ItemStack(Items.STONE, stored));
      }
      var original = new ItemStack(item, 7); world.trace.clear();
      var remainder = HopperBlockEntity.addItem(null, inventory, original, null);
      System.out.println("ordinary-transfer\t" + name + "\t" + stored + "\t" + BuiltInRegistries.ITEM.getKey(item) + "\t" + target.saveCustomOnly(access) + "\t" + BookShelfHookVectors.stack(remainder) + "\t" + String.join(";", world.trace));
    }
  }
}
