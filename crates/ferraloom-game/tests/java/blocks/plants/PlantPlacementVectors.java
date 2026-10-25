import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.DirectionalPlaceContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;

/** Native placement and support checks on the concrete Level fixture. */
public final class PlantPlacementVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final Direction[] HORIZONTAL = {Direction.NORTH, Direction.EAST, Direction.SOUTH, Direction.WEST};
    static void put(MutationFixture level, BlockPos pos, BlockState state) {
        level.getChunkAt(pos).setBlockState(pos, state, 816);
    }
    static void recordPlacement(MutationFixture level, Block block, String inputs, Direction clicked) {
        var context = new DirectionalPlaceContext(level, POS, clicked, ItemStack.EMPTY, Direction.UP);
        level.getRandom().setSeed(1234);
        var result = block.getStateForPlacement(context);
        System.out.println(inputs + "\t" + clicked.ordinal() + "\t" + (result == null ? -1 : Block.getId(result)) + "\t" + level.getRandom().nextLong());
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        var level = new MutationFixture(registries);
        for (var block : List.of(Blocks.COCOA, Blocks.SHELF_MUSHROOM)) {
            for (var state : block.getStateDefinition().getPossibleStates()) {
                for (var supportBlock : List.of(Blocks.AIR, Blocks.STONE, Blocks.JUNGLE_LOG, Blocks.OAK_LOG, Blocks.JUNGLE_LEAVES, Blocks.OAK_SLAB, Blocks.OAK_STAIRS, Blocks.OAK_FENCE, Blocks.FARMLAND, Blocks.WATER)) {
                    for (var support : supportBlock.getStateDefinition().getPossibleStates()) {
                        for (var direction : Direction.values()) {
                            for (var face : Direction.values()) put(level, POS.relative(face), Blocks.AIR.defaultBlockState());
                            put(level, POS.relative(direction), support);
                            level.getRandom().setSeed(1234);
                            System.out.println("survive\t" + Block.getId(state) + "\t" + Block.getId(support) + "\t" + direction.ordinal() + "\t" + state.canSurvive(level, POS) + "\t" + level.getRandom().nextLong());
                        }
                    }
                }
            }
            var supports = List.of(Blocks.JUNGLE_LOG, Blocks.STONE, Blocks.OAK_SLAB);
            for (int mask = 0; mask < 16; mask++) for (int mode = 0; mode < supports.size(); mode++) for (var clicked : Direction.values()) {
                for (int i = 0; i < HORIZONTAL.length; i++) put(level, POS.relative(HORIZONTAL[i]), ((mask & (1 << i)) != 0 ? supports.get(mode) : Blocks.AIR).defaultBlockState());
                put(level, POS, Blocks.AIR.defaultBlockState());
                recordPlacement(level, block, "wall\t" + BuiltInRegistries.BLOCK.getId(block) + "\t" + mask + "\t" + mode, clicked);
            }
        }
        var fluids = new ArrayList<BlockState>();
        fluids.add(Blocks.AIR.defaultBlockState());
        fluids.addAll(Blocks.WATER.getStateDefinition().getPossibleStates());
        fluids.addAll(Blocks.LAVA.getStateDefinition().getPossibleStates());
        fluids.add(Blocks.OAK_SLAB.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED, true));
        for (var block : List.of(Blocks.MANGROVE_ROOTS, Blocks.SCULK_SENSOR, Blocks.CALIBRATED_SCULK_SENSOR, Blocks.SCULK_SHRIEKER)) {
            for (var replaced : fluids) for (var clicked : Direction.values()) {
                put(level, POS, replaced);
                recordPlacement(level, block, "wet\t" + BuiltInRegistries.BLOCK.getId(block) + "\t" + Block.getId(replaced), clicked);
            }
        }
    }
}
