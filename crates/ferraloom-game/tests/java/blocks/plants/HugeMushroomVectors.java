import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.DirectionalPlaceContext;
import net.minecraft.world.level.block.*;

/** Real HugeMushroomBlock virtuals on the concrete native Level fixture. */
public final class HugeMushroomVectors {
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        var level = new MutationFixture(registries);
        var pos = new BlockPos(8, 5, 8);
        var blocks = List.of(Blocks.BROWN_MUSHROOM_BLOCK, Blocks.RED_MUSHROOM_BLOCK, Blocks.MUSHROOM_STEM);
        for (var block : blocks) {
            for (var state : block.getStateDefinition().getPossibleStates()) {
                for (var neighbor : List.of(Blocks.AIR, Blocks.BROWN_MUSHROOM_BLOCK, Blocks.RED_MUSHROOM_BLOCK, Blocks.MUSHROOM_STEM)) {
                    for (var direction : Direction.values()) {
                        level.getRandom().setSeed(1234);
                        var result = state.updateShape(level, level, pos, direction, pos.relative(direction), neighbor.defaultBlockState(), level.getRandom());
                        System.out.println("update\t" + Block.getId(state) + "\t" + Block.getId(neighbor.defaultBlockState()) + "\t" + direction.ordinal() + "\t" + Block.getId(result) + "\t" + level.getRandom().nextLong());
                    }
                }
                var transforms = new ArrayList<String>();
                for (var rotation : Rotation.values()) transforms.add(Integer.toString(Block.getId(state.rotate(rotation))));
                for (var mirror : Mirror.values()) transforms.add(Integer.toString(Block.getId(state.mirror(mirror))));
                System.out.println("transform\t" + Block.getId(state) + "\t" + String.join("\t", transforms));
            }
            for (int mask = 0; mask < 64; mask++) for (int mode = 0; mode < 2; mode++) for (var clicked : Direction.values()) {
                for (var direction : Direction.values()) {
                    var neighbor = (mask & (1 << direction.ordinal())) != 0 ? block : mode == 0 ? Blocks.AIR : blocks.get((blocks.indexOf(block) + 1) % 3);
                    var at = pos.relative(direction);
                    level.getChunkAt(at).setBlockState(at, neighbor.defaultBlockState(), 816);
                }
                level.getChunkAt(pos).setBlockState(pos, Blocks.AIR.defaultBlockState(), 816);
                var context = new DirectionalPlaceContext(level, pos, clicked, ItemStack.EMPTY, Direction.UP);
                level.getRandom().setSeed(1234);
                var state = block.getStateForPlacement(context);
                System.out.println("place\t" + BuiltInRegistries.BLOCK.getId(block) + "\t" + mask + "\t" + mode + "\t" + clicked.ordinal() + "\t" + Block.getId(state) + "\t" + level.getRandom().nextLong());
            }
        }
    }
}
