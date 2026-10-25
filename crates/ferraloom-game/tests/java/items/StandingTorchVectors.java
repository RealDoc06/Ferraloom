import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;

/** BaseTorchBlock survival and shape virtuals on the native concrete Level. */
public final class StandingTorchVectors {
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        var level = new MutationFixture(registries);
        var pos = new BlockPos(8, 64, 8);
        var supports = new ArrayList<BlockState>();
        for (var block : List.of(Blocks.AIR, Blocks.STONE, Blocks.SOUL_SAND, Blocks.OAK_FENCE,
                Blocks.COBBLESTONE_WALL, Blocks.GLASS, Blocks.WATER, Blocks.OAK_LEAVES,
                Blocks.DIRT_PATH, Blocks.FARMLAND, Blocks.CACTUS, Blocks.HONEY_BLOCK)) {
            supports.add(block.defaultBlockState());
        }
        for (var type : SlabType.values()) supports.add(Blocks.STONE_SLAB.defaultBlockState().setValue(BlockStateProperties.SLAB_TYPE, type));
        for (int layers : new int[]{1, 8}) supports.add(Blocks.SNOW.defaultBlockState().setValue(BlockStateProperties.LAYERS, layers));
        for (var block : List.of(Blocks.TORCH, Blocks.SOUL_TORCH)) for (var support : supports) {
            level.getChunkAt(pos.below()).setBlockState(pos.below(), support, 816);
            var state = block.defaultBlockState();
            var result = new ArrayList<String>();
            result.add("torch"); result.add(Integer.toString(Block.getId(state)));
            result.add(Integer.toString(Block.getId(support)));
            result.add(Boolean.toString(state.canSurvive(level, pos)));
            level.getRandom().setSeed(1234);
            for (var direction : Direction.values()) {
                var at = pos.relative(direction);
                result.add(Integer.toString(Block.getId(state.updateShape(level, level, pos, direction, at, level.getBlockState(at), level.getRandom()))));
            }
            result.add(Long.toString(level.getRandom().nextLong()));
            System.out.println(String.join("\t", result));
        }
    }
}
