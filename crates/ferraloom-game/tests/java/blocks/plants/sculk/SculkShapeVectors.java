import java.util.*;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.*;

/** Native block virtuals and actual LevelTicks deduplication on a concrete Level. */
public final class SculkShapeVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final class ShapeLevel extends MutationFixture {
        LevelTicks<Fluid> fluids;
        LevelChunkTicks<Fluid> container;
        ShapeLevel(RegistryAccess registries) { super(registries); }
        @Override public long getGameTime() { return 0; } // controlled native fixture clock
        void reset() {
            fluids = new LevelTicks<>(p -> true);
            container = new LevelChunkTicks<>();
            fluids.addContainer(new net.minecraft.world.level.ChunkPos(POS.getX() >> 4, POS.getZ() >> 4), container);
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
        for (var block : List.of(Blocks.SCULK_SENSOR, Blocks.CALIBRATED_SCULK_SENSOR, Blocks.SCULK_SHRIEKER, Blocks.SCULK_CATALYST, Blocks.SCULK_VEIN)) {
            for (var state : block.getStateDefinition().getPossibleStates()) {
                for (var neighbor : List.of(Blocks.AIR, Blocks.STONE)) {
                    for (var direction : Direction.values()) {
                        level.getChunkAt(POS.relative(direction)).setBlockState(POS.relative(direction), neighbor.defaultBlockState(), 816);
                    }
                    for (int direction = -1; direction < 6; direction++) {
                        level.reset();
                        var result = direction < 0
                            ? Block.updateFromNeighbourShapes(state, level, POS)
                            : state.updateShape(level, level, POS, Direction.values()[direction], POS.relative(Direction.values()[direction]), neighbor.defaultBlockState(), level.getRandom());
                        System.out.println("shape\t" + Block.getId(state) + "\t" + Block.getId(neighbor.defaultBlockState()) + "\t" + direction + "\t" + Block.getId(result) + "\t" + level.ticks() + "\t" + level.getRandom().nextLong());
                    }
                }
            }
        }
    }
}
