import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

/** Actual item-subclass virtuals, with only the surrounding concrete Level controlled. */
public final class PlacementVectors {
    static Method method(Item item, String name, Class<?>... parameters) throws Exception {
        for (Class<?> type = item.getClass(); type != null; type = type.getSuperclass()) {
            try { var method = type.getDeclaredMethod(name, parameters); method.setAccessible(true); return method; }
            catch (NoSuchMethodException missing) { }
        }
        throw new AssertionError(name);
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var pos = BlockItemVectors.POS;
        var level = new BlockItemVectors.Fixture(registries);
        for (var item : List.of(Items.TORCH, Items.SOUL_TORCH, Items.OAK_SIGN, Items.OAK_HANGING_SIGN, Items.TUBE_CORAL_FAN, Items.DEAD_TUBE_CORAL_FAN)) {
            for (int mask = 0; mask < 64; mask++) for (var direction : Direction.values()) for (var clicked : Direction.values()) {
                for (var face : Direction.values()) level.put(pos.relative(face), ((mask & (1 << face.ordinal())) == 0 ? Blocks.AIR : Blocks.STONE).defaultBlockState());
                level.put(pos, Blocks.AIR.defaultBlockState());
                var context = new DirectionalPlaceContext(level, pos, direction, new ItemStack(item), clicked);
                level.getRandom().setSeed(1234);
                var state = (BlockState) method(item, "getPlacementState", BlockPlaceContext.class).invoke(item, context);
                System.out.println("standing\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + mask + "\t" + direction.ordinal() + "\t" + clicked.ordinal() + "\t" + (state == null ? -1 : Block.getId(state)) + "\t" + level.getRandom().nextLong());
            }
        }
        for (var item : List.of(Items.SUNFLOWER, Items.SMALL_DRIPLEAF, Items.OAK_DOOR)) {
            for (var above : List.of(Blocks.AIR.defaultBlockState(), Blocks.STONE.defaultBlockState(), Blocks.WATER.defaultBlockState(), Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL, 1), Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL, 8), Blocks.OAK_LEAVES.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED, true))) {
                level = new BlockItemVectors.Fixture(registries);
                level.put(pos.below(), Blocks.DIRT.defaultBlockState()); level.put(pos.above(), above);
                var context = new DirectionalPlaceContext(level, pos, Direction.NORTH, new ItemStack(item), Direction.UP);
                level.getRandom().setSeed(1234);
                boolean result = (boolean) method(item, "placeBlock", BlockPlaceContext.class, BlockState.class).invoke(item, context, ((BlockItem)item).getBlock().defaultBlockState());
                System.out.println("double\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + Block.getId(above) + "\t" + result + "\t" + Block.getId(level.getBlockState(pos)) + "\t" + Block.getId(level.getBlockState(pos.above())) + "\t" + level.getRandom().nextLong());
            }
        }
        for (var face : List.of(Direction.UP, Direction.NORTH)) for (int chain = 0; chain <= 8; chain++) for (boolean blocked : new boolean[]{false, true}) {
            level = new BlockItemVectors.Fixture(registries);
            var direction = face == Direction.UP ? Direction.NORTH : Direction.UP;
            for (int i = 0; i <= chain; i++) level.put(pos.relative(direction, i), Blocks.SCAFFOLDING.defaultBlockState());
            if (blocked) level.put(pos.relative(direction, chain + 1), Blocks.STONE.defaultBlockState());
            var context = new BlockPlaceContext(new BlockItemVectors.Context(level, new ItemStack(Items.SCAFFOLDING), face));
            level.getRandom().setSeed(1234);
            var result = ((BlockItem)Items.SCAFFOLDING).updatePlacementContext(context);
            System.out.println("scaffolding\t" + face.ordinal() + "\t" + chain + "\t" + blocked + "\t" + (result == null ? "none" : result.getClickedPos().toShortString() + ":" + result.getClickedFace().ordinal()) + "\t" + level.getRandom().nextLong());
        }
        for (var item : List.of(Items.LILY_PAD, Items.FROGSPAWN)) {
            var result = item.useOn(new BlockItemVectors.Context(level, new ItemStack(item), Direction.UP));
            System.out.println("water-use-on\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + (result == net.minecraft.world.InteractionResult.PASS));
        }
    }
}
