import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;

/** Actual LightBlock update virtuals with the shared native LevelTicks fixture. */
public final class LightUpdateVectors {
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        var level = new SculkShapeVectors.ShapeLevel(registries);
        var pos = SculkShapeVectors.POS;
        for (var state : Blocks.LIGHT.getStateDefinition().getPossibleStates()) {
            for (var neighbor : List.of(Blocks.AIR, Blocks.WATER)) {
                for (var direction : Direction.values()) level.getChunkAt(pos.relative(direction)).setBlockState(pos.relative(direction), neighbor.defaultBlockState(), 816);
                for (int direction = -1; direction < 6; direction++) {
                    level.reset();
                    var result = direction < 0 ? Block.updateFromNeighbourShapes(state, level, pos)
                        : state.updateShape(level, level, pos, Direction.values()[direction], pos.relative(Direction.values()[direction]), neighbor.defaultBlockState(), level.getRandom());
                    System.out.println("light\t" + Block.getId(state) + "\t" + Block.getId(neighbor.defaultBlockState()) + "\t" + direction + "\t" + Block.getId(result) + "\t" + level.ticks() + "\t" + level.getRandom().nextLong());
                }
            }
        }
    }
}
