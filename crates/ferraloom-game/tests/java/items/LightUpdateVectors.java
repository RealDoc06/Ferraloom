import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.item.*;
import net.minecraft.world.phys.shapes.*;

/** Actual LightBlock update virtuals with the shared native LevelTicks fixture. */
public final class LightUpdateVectors {
    static final class HeldContext extends EntityCollisionContext {
        HeldContext(ItemStack held) { super(false, false, 0.0, held, false, null); }
    }
    static boolean full(VoxelShape shape) {
        boolean full = Block.isShapeFullBlock(shape);
        if (!full && !shape.isEmpty()) throw new AssertionError("light shape is neither cube nor empty");
        return full;
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
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
        var contexts = List.of(CollisionContext.empty(), new HeldContext(ItemStack.EMPTY),
            new HeldContext(new ItemStack(Items.LIGHT)), new HeldContext(new ItemStack(Items.STONE)),
            new HeldContext(new ItemStack(Items.BONE_MEAL)), CollisionContext.positionContext(2.0),
            CollisionContext.placementContext(null));
        for (var state : Blocks.LIGHT.getStateDefinition().getPossibleStates()) {
            for (int context = 0; context < contexts.size(); context++) {
                level.getRandom().setSeed(1234);
                var outline = state.getShape(level, pos, contexts.get(context));
                var collision = state.getCollisionShape(level, pos, contexts.get(context));
                var plainOutline = state.getShape(level, pos);
                var cachedCollision = state.getCollisionShape(level, pos);
                System.out.println("light-shape\t" + Block.getId(state) + "\t" + context + "\t"
                    + full(outline) + "\t" + full(collision) + "\t" + full(plainOutline) + "\t"
                    + full(cachedCollision) + "\t" + level.getRandom().nextLong());
            }
        }
    }
}
