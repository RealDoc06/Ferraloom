import java.util.*;
import java.util.function.Predicate;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.sounds.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.BlockItemStateProperties;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.entity.BlockEntityTypes;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.*;

/** Real ItemStack#useOn and BlockItem#place in a controlled, empty native Level. */
public final class BlockItemVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final class Context extends UseOnContext {
        Context(Fixture level, ItemStack stack, Direction face) {
            super(level, null, InteractionHand.MAIN_HAND, stack, new BlockHitResult(Vec3.atCenterOf(POS), face, POS, false));
        }
    }
    static final class Fixture extends MutationFixture {
        final List<String> actions = new ArrayList<>();
        Fixture(RegistryAccess registries) { super(registries); }
        // The fixture contains no entities. The native isUnobstructed loop and
        // context-dependent block collision methods still execute normally.
        @Override public List<Entity> getEntities(Entity except, AABB box, Predicate<? super Entity> predicate) {
            actions.add("entities");
            return List.of();
        }
        @Override public void playSeededSound(Entity except, double x, double y, double z, Holder sound, SoundSource source, float volume, float pitch, long seed) {
            actions.add("sound:" + ((SoundEvent) sound.value()).location() + ":" + Float.floatToRawIntBits(volume) + ":" + Float.floatToRawIntBits(pitch));
        }
        @Override public void gameEvent(Holder event, Vec3 at, GameEvent.Context context) {
            actions.add("event:" + ((net.minecraft.resources.ResourceKey<?>) event.unwrapKey().orElseThrow()).identifier() + ":" + Block.getId(context.affectedState()));
        }
        void put(BlockPos pos, BlockState state) {
            getChunkAt(pos).setBlockState(pos, state, 816);
        }
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        for (var item : List.of(Items.STONE, Items.DIRT, Items.GLASS, Items.BROWN_MUSHROOM_BLOCK, Items.RED_MUSHROOM_BLOCK)) {
            for (var face : Direction.values()) for (int occupied = 0; occupied < 3; occupied++) for (boolean separate : new boolean[]{false, true}) for (int properties = 0; properties < 3; properties++) {
                var level = new Fixture(registries);
                for (var direction : Direction.values()) level.put(POS.relative(direction), (occupied == 2 ? Blocks.STONE : Blocks.AIR).defaultBlockState());
                level.put(POS, (occupied == 0 ? Blocks.AIR : Blocks.STONE).defaultBlockState());
                var receiver = new ItemStack(item, 3);
                var held = separate ? new ItemStack(Items.DIRT, 7) : receiver;
                if (properties != 0) held.set(DataComponents.BLOCK_STATE, new BlockItemStateProperties(Map.of("north", properties == 1 ? "false" : "invalid")));
                level.actions.clear(); level.trace.clear(); level.getRandom().setSeed(1234);
                var result = receiver.useOn(new Context(level, held, face));
                String outcome = result instanceof InteractionResult.Success success
                    ? "success:" + success.swingSource() + ":" + success.wasItemInteraction() + ":" + (success.heldItemTransformedTo() != null)
                    : result == InteractionResult.FAIL ? "fail" : result == InteractionResult.PASS ? "pass" : "other";
                System.out.println("place\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + face.ordinal() + "\t" + occupied + "\t" + separate + "\t" + properties
                    + "\t" + outcome + "\t" + receiver.getCount() + "\t" + held.getCount()
                    + "\t" + Block.getId(level.getBlockState(POS)) + "\t" + Block.getId(level.getBlockState(POS.relative(face)))
                    + "\t" + String.join(";", level.actions) + "\t" + level.getRandom().nextLong());
            }
        }
        // Empty collision shapes still require their native support checks.
        // Exercise both replace-clicked and adjacent placement, with a context
        // receiver that can differ from the ItemStack#useOn receiver.
        for (var item : List.of(Items.POPPY, Items.OAK_SAPLING, Items.FERN, Items.CRIMSON_ROOTS, Items.DEAD_BUSH)) {
            for (var support : List.of(Blocks.AIR, Blocks.STONE, Blocks.DIRT, Blocks.GRASS_BLOCK, Blocks.FARMLAND, Blocks.SAND, Blocks.SOUL_SOIL, Blocks.CRIMSON_NYLIUM)) {
                for (int adjacent = 0; adjacent < 2; adjacent++) for (boolean separate : new boolean[]{false, true}) {
                    var level = new Fixture(registries);
                    level.put(POS.below(), support.defaultBlockState());
                    level.put(POS, (adjacent == 0 ? Blocks.AIR : support).defaultBlockState());
                    var receiver = new ItemStack(item, 3);
                    var held = separate ? new ItemStack(Items.DIRT, 7) : receiver;
                    level.actions.clear(); level.trace.clear(); level.getRandom().setSeed(1234);
                    var result = receiver.useOn(new Context(level, held, Direction.UP));
                    String outcome = result instanceof InteractionResult.Success success
                        ? "success:" + success.swingSource() + ":" + success.wasItemInteraction() + ":" + (success.heldItemTransformedTo() != null)
                        : result == InteractionResult.FAIL ? "fail" : result == InteractionResult.PASS ? "pass" : "other";
                    System.out.println("plant\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + Direction.UP.ordinal() + "\t" + adjacent + "\t" + separate + "\t" + Block.getId(support.defaultBlockState())
                        + "\t" + outcome + "\t" + receiver.getCount() + "\t" + held.getCount()
                        + "\t" + Block.getId(level.getBlockState(POS)) + "\t" + Block.getId(level.getBlockState(POS.above()))
                        + "\t" + String.join(";", level.actions) + "\t" + level.getRandom().nextLong());
                }
            }
        }
        var properties = new LinkedHashMap<Block, String>();
        properties.put(Blocks.COCOA, "age");
        properties.put(Blocks.SNOW, "layers");
        properties.put(Blocks.WHEAT, "age");
        properties.put(Blocks.RED_MUSHROOM_BLOCK, "north");
        properties.put(Blocks.OAK_STAIRS, "facing");
        properties.put(Blocks.STONE, "missing");
        for (var entry : properties.entrySet()) for (var state : entry.getKey().getStateDefinition().getPossibleStates()) {
            for (var value : List.of("+1", "01", "١", "１", "𝟙", "1", "-0", "+0", "0002", "2", "2147483647", "2147483648", "-2147483648", "-2147483649", "true", "True", "false", "north", "east", "UP", "unknown", "")) {
                var patch = new BlockItemStateProperties(Map.of(entry.getValue(), value));
                System.out.println("property\t" + Block.getId(state) + "\t" + entry.getValue() + "\t" + value + "\t" + Block.getId(patch.apply(state)));
            }
        }
        for (var state : Blocks.COCOA.getStateDefinition().getPossibleStates()) {
            for (var values : List.of(List.of("1", "invalid"), List.of("invalid", "+1"), List.of("1", "2"), List.of("+1", "١"), List.of("invalid", "invalid"), List.of("2", "-0"))) {
                var buffer = new net.minecraft.network.FriendlyByteBuf(io.netty.buffer.Unpooled.buffer());
                try {
                    buffer.writeVarInt(2);
                    for (var value : values) { buffer.writeUtf("age"); buffer.writeUtf(value); }
                    var decoded = BlockItemStateProperties.STREAM_CODEC.decode(buffer);
                    System.out.println("duplicate\t" + Block.getId(state) + "\t" + values.get(0) + "\t" + values.get(1) + "\t" + Block.getId(decoded.apply(state)));
                } finally { buffer.release(); }
            }
        }
        for (int kind = 0; kind < 3; kind++) for (int patch = 0; patch < 5; patch++) {
            var level = new Fixture(registries);
            var block = List.of(Blocks.CHEST, Blocks.BARREL, Blocks.FURNACE).get(kind);
            var type = List.of(BlockEntityTypes.CHEST, BlockEntityTypes.BARREL, BlockEntityTypes.FURNACE).get(kind);
            level.put(POS, block.defaultBlockState());
            var stack = new ItemStack(block.asItem(), 3);
            var tag = new net.minecraft.nbt.CompoundTag();
            if (patch == 1 || patch == 2) tag.putString("CustomName", "native");
            if (patch == 3) tag.putInt("ignored", 1);
            if (patch == 4) tag.putString("id", "minecraft:furnace");
            stack.set(DataComponents.BLOCK_ENTITY_DATA, net.minecraft.world.item.component.TypedEntityData.of(patch == 2 ? BlockEntityTypes.SIGN : type, tag));
            for (int repeat = 0; repeat < 2; repeat++) {
                boolean result = BlockItem.updateCustomBlockEntityTag(level, null, POS, stack);
                var entity = level.getBlockEntity(POS);
                var name = ((net.minecraft.world.Nameable) entity).getCustomName();
                System.out.println("be\t" + kind + "\t" + patch + "\t" + repeat + "\t" + result + "\t" + (name == null ? "" : name.getString()));
            }
        }
    }
}
