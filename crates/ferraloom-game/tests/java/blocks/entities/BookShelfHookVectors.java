import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.resources.*;
import net.minecraft.sounds.*;
import net.minecraft.stats.*;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.*;

/** Real bookshelf virtuals on a concrete Level; only external output is observed. */
public final class BookShelfHookVectors {
  static final BlockPos POS = new BlockPos(8, 64, 8);
  static HolderLookup.Provider context;
  static String stack(ItemStack stack) { return ItemStack.OPTIONAL_CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE), stack).getOrThrow().toString(); }
  static String itemList(List<ItemStack> items) { return String.join("#", items.stream().map(BookShelfHookVectors::stack).toList()); }
  static final class World extends NeighborFixture {
    ChiseledBookShelfBlockEntity shelf;
    final List<String> trace = new ArrayList<>();
    final List<ItemEntity> drops = new ArrayList<>();
    int ids;
    World(RegistryAccess access) { super(access, 100); }
    @Override public int getNextEntityId() { return ++ids; }
    @Override public BlockState getBlockState(BlockPos pos) { return blocks.getOrDefault(pos, Blocks.AIR.defaultBlockState()); }
    @Override public FluidState getFluidState(BlockPos pos) { return getBlockState(pos).getFluidState(); }
    @Override public BlockEntity getBlockEntity(BlockPos pos) { return pos.equals(POS) ? shelf : null; }
    @Override public boolean setBlock(BlockPos pos, BlockState state, int flags, int limit) {
      var old = blocks.put(pos.immutable(), state);
      if (old == state) return false;
      if (pos.equals(POS)) shelf.setBlockState(state);
      trace.add("u:" + Block.getId(state)); return true;
    }
    @Override public void blockEntityChanged(BlockPos pos) { trace.add("changed"); }
    @Override public void updateNeighbourForOutputSignal(BlockPos pos, Block block) { trace.add("analog"); }
    @Override public void gameEvent(Holder event, Vec3 position, GameEvent.Context context) {
      trace.add("g:" + BuiltInRegistries.GAME_EVENT.getId((GameEvent)event.value()) + ":" + (context.sourceEntity() == null ? 0 : context.sourceEntity().getId()) + ":" + (context.affectedState() == null ? -1 : Block.getId(context.affectedState())));
    }
    @Override public void playSeededSound(Entity except, double x, double y, double z, Holder sound, SoundSource source, float volume, float pitch, long seed) {
      trace.add("s:" + ((SoundEvent)sound.value()).location() + ":" + x + "," + y + "," + z + ":" + volume + ":" + pitch);
    }
    @Override public boolean addFreshEntity(Entity entity) { if (entity instanceof ItemEntity item) drops.add(item); return true; }
    void prepare(int mask) {
      var state = Blocks.CHISELED_BOOKSHELF.defaultBlockState();
      for (int i = 0; i < 6; i++) state = state.setValue(ChiseledBookShelfBlock.SLOT_OCCUPIED_PROPERTIES.get(i), (mask & (1 << i)) != 0);
      blocks.clear(); blocks.put(POS, state);
      shelf = new ChiseledBookShelfBlockEntity(POS, state); shelf.setLevel(this);
      for (int i = 0; i < 6; i++) if ((mask & (1 << i)) != 0) shelf.getItems().set(i, new ItemStack(Items.BOOK, i + 1));
      trace.clear(); drops.clear();
    }
  }
  static final class Actor extends Player {
    int used;
    Actor(Level level) { super(level, new com.mojang.authlib.GameProfile(new UUID(1, 2), "bookshelf")); setId(1); setPos(8.5, 65, 10.5); }
    @Override public GameType gameMode() { return GameType.SURVIVAL; }
    @Override public void awardStat(Stat<?> stat, int amount) { if (stat.getType() == Stats.ITEM_USED) used += amount; }
  }
  static BlockHitResult hit(Direction face, float x, float y, float z) { return new BlockHitResult(new Vec3(POS.getX() + (double)x, POS.getY() + (double)y, POS.getZ() + (double)z), face, POS, false); }
  static String result(InteractionResult result) {
    if (result == InteractionResult.SUCCESS) return "success";
    if (result == InteractionResult.CONSUME) return "consume";
    if (result == InteractionResult.TRY_WITH_EMPTY_HAND) return "empty-hand";
    if (result == InteractionResult.PASS) return "pass";
    throw new AssertionError(result);
  }
  static void hits() {
    var coordinates = new ArrayList<Float>(List.of(-0.5f, -0.0f, 0f, 0.25f, 0.5f, 0.75f, 1f, 1.5f));
    for (float boundary : new float[]{1f / 3f, 0.5f, 2f / 3f}) for (float coordinate : new float[]{Math.nextDown(boundary), boundary, Math.nextUp(boundary)}) coordinates.add(coordinate);
    for (var facing : Direction.Plane.HORIZONTAL) for (var face : Direction.values()) for (float horizontal : coordinates) for (float vertical : coordinates) {
      var state = Blocks.CHISELED_BOOKSHELF.defaultBlockState().setValue(ChiseledBookShelfBlock.FACING, facing);
      var hit = hit(face, horizontal, vertical, horizontal);
      int slot = ((SelectableSlotContainer)Blocks.CHISELED_BOOKSHELF).getHitSlot(hit, facing).orElse(-1);
      System.out.println("shelf-hit\t" + Block.getId(state) + "\t" + face.get3DDataValue() + "\t" + Integer.toUnsignedString(Float.floatToRawIntBits(horizontal), 16) + "\t" + Integer.toUnsignedString(Float.floatToRawIntBits(vertical), 16) + "\t" + slot);
    }
  }
  public static void main(String[] args) throws Exception {
    var output = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
    var worldRegistries = NeighborVectors.load(); var builtin = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var access = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtin.registries(), worldRegistries.registries())).freeze();
    context = access; BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    var world = new World(access);
    hits();
    for (int mask = 0; mask < 64; mask++) for (int slot = 0; slot < 6; slot++) for (int operation = 0; operation < 10; operation++) {
      world.prepare(mask); var original = world.shelf.getItem(slot); ItemStack returned = ItemStack.EMPTY;
      var input = switch (operation) { case 0 -> new ItemStack(Items.BOOK, 3); case 1 -> new ItemStack(Items.ENCHANTED_BOOK, 2); case 2 -> new ItemStack(Items.STONE, 1); case 3 -> new ItemStack(Items.BOOK, 0); default -> ItemStack.EMPTY; };
      if (operation < 5) world.shelf.setItem(slot, input);
      else if (operation < 9) returned = world.shelf.removeItem(slot, new int[]{-7, 0, 1, 99}[operation - 5]);
      else returned = world.shelf.removeItemNoUpdate(slot);
      System.out.println("shelf-hook\t" + mask + "\t" + slot + "\t" + operation + "\t" + world.shelf.saveCustomOnly(context) + "\t" + stack(returned) + "\t" + (returned == original) + "\t" + Block.getId(world.getBlockState(POS)) + "\t" + String.join(";", world.trace));
    }
    for (boolean remove : new boolean[]{false, true}) for (int slot = 0; slot < 6; slot++) for (boolean occupied : new boolean[]{false, true}) for (int item = 0; item < 4; item++) for (boolean infinite : new boolean[]{false, true}) for (boolean wrongFace : new boolean[]{false, true}) {
      world.prepare(occupied ? 1 << slot : 0); var actor = new Actor(world); actor.getAbilities().instabuild = infinite;
      var input = switch (item) { case 0 -> new ItemStack(Items.BOOK, 3); case 1 -> new ItemStack(Items.ENCHANTED_BOOK, 3); case 2 -> new ItemStack(Items.STONE, 3); default -> ItemStack.EMPTY; };
      actor.setItemInHand(InteractionHand.MAIN_HAND, input);
      float horizontal = (slot % 3 + 0.5f) / 3f; float vertical = slot < 3 ? 0.75f : 0.25f;
      var hit = hit(wrongFace ? Direction.SOUTH : Direction.NORTH, 1f - horizontal, vertical, 0f);
      var state = world.getBlockState(POS);
      var outcome = remove ? state.useWithoutItem(world, actor, hit) : state.useItemOn(input, world, actor, InteractionHand.MAIN_HAND, hit);
      var inventory = new ArrayList<ItemStack>(); for (int i = 0; i < 36; i++) inventory.add(actor.getInventory().getItem(i));
      System.out.println("shelf-use\t" + remove + "\t" + slot + "\t" + occupied + "\t" + item + "\t" + infinite + "\t" + wrongFace + "\t" + result(outcome) + "\t" + world.shelf.saveCustomOnly(context) + "\t" + stack(actor.getMainHandItem()) + "\t" + actor.used + "\t" + Block.getId(world.getBlockState(POS)) + "\t" + String.join(";", world.trace) + "\t" + itemList(inventory));
    }
    for (int last : new int[]{-1, 0, 5, 6, -9, Integer.MIN_VALUE, Integer.MAX_VALUE}) for (var direction : Direction.values()) {
      world.prepare(0);
      world.shelf.loadWithComponents(net.minecraft.world.level.storage.TagValueInput.create(net.minecraft.util.ProblemReporter.DISCARDING, context, TagParser.parseCompoundFully("{last_interacted_slot:" + last + "}")));
      System.out.println("shelf-signal\t" + last + "\t" + direction.get3DDataValue() + "\t" + world.getBlockState(POS).getAnalogOutputSignal(world, POS, direction));
    }
  }
}
