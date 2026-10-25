import java.util.*;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.*;

/** Actual sign/banner support/update virtuals with native LevelTicks deduplication. */
public final class SignSupportVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final class ShapeLevel extends MutationFixture {
        LevelTicks<Fluid> fluids;
        LevelChunkTicks<Fluid> container;
        ShapeLevel(RegistryAccess registries) { super(registries); }
        @Override public long getGameTime() { return 0; }
        void reset() {
            fluids = new LevelTicks<>(p -> true);
            container = new LevelChunkTicks<>();
            fluids.addContainer(new net.minecraft.world.level.ChunkPos(0, 0), container);
            getRandom().setSeed(1234);
        }
        @Override public void scheduleTick(BlockPos pos, Fluid fluid, int delay) {
            fluids.schedule(createTick(pos, fluid, delay));
        }
        String ticks() {
            return container.getAll().map(t -> BuiltInRegistries.FLUID.getKey(t.type()) + "," + t.triggerTick() + "," + t.priority().getValue()).collect(Collectors.joining(";"));
        }
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        var level = new ShapeLevel(registries);
        var supports = new ArrayList<BlockState>();
        for (var block : List.of(Blocks.AIR, Blocks.STONE, Blocks.SOUL_SAND, Blocks.OAK_FENCE,
                Blocks.COBBLESTONE_WALL, Blocks.GLASS, Blocks.WATER, Blocks.OAK_LEAVES,
                Blocks.DIRT_PATH, Blocks.FARMLAND, Blocks.CACTUS, Blocks.HONEY_BLOCK,
                Blocks.BAMBOO, Blocks.OAK_HANGING_SIGN, Blocks.OAK_WALL_HANGING_SIGN)) {
            supports.add(block.defaultBlockState());
        }
        for (var type : SlabType.values()) supports.add(Blocks.STONE_SLAB.defaultBlockState().setValue(BlockStateProperties.SLAB_TYPE, type));
        for (int layers : new int[]{1, 8}) supports.add(Blocks.SNOW.defaultBlockState().setValue(BlockStateProperties.LAYERS, layers));
        for (var block : List.of(Blocks.OAK_SIGN, Blocks.OAK_WALL_SIGN, Blocks.OAK_HANGING_SIGN,
                Blocks.OAK_WALL_HANGING_SIGN, Blocks.BANNER.pick(net.minecraft.world.item.DyeColor.WHITE), Blocks.WALL_BANNER.pick(net.minecraft.world.item.DyeColor.WHITE))) {
            for (var state : block.getStateDefinition().getPossibleStates()) for (var support : supports) {
                for (var direction : Direction.values()) level.getChunkAt(POS.relative(direction)).setBlockState(POS.relative(direction), support, 816);
                for (int direction = -1; direction < 6; direction++) {
                    level.reset();
                    boolean survives = state.canSurvive(level, POS);
                    var result = direction < 0 ? Block.updateFromNeighbourShapes(state, level, POS)
                        : state.updateShape(level, level, POS, Direction.values()[direction], POS.relative(Direction.values()[direction]), support, level.getRandom());
                    System.out.println("sign\t" + Block.getId(state) + "\t" + Block.getId(support) + "\t" + direction + "\t" + survives + "\t" + Block.getId(result) + "\t" + level.ticks() + "\t" + level.getRandom().nextLong());
                }
            }
        }
    }
}
