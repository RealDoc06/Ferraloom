import java.lang.reflect.Proxy;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.SnowLayerBlock;
import net.minecraft.world.phys.AABB;
import static org.mockito.Mockito.*;

/** Observes the real Block.pushEntitiesUp used by ServerLevel.tickPrecipitation. */
public final class SnowPushVectors {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var rows = new StringBuilder();
        for (int layers = 1; layers < 8; layers++) {
            final double oldY = (layers - 1) / 8.0;
            final double[] movedY = {0};
            Entity entity = mock(Entity.class);
            when(entity.getBoundingBox()).thenReturn(new AABB(8.01, oldY, 8.01, 8.99, oldY + 0.98, 8.99));
            doAnswer(call -> {movedY[0] += (Double)call.getArgument(1); return null;})
                .when(entity).teleportRelative(anyDouble(), anyDouble(), anyDouble());
            LevelAccessor level = (LevelAccessor)Proxy.newProxyInstance(
                SnowPushVectors.class.getClassLoader(), new Class<?>[]{LevelAccessor.class},
                (proxy, method, values) -> {
                    if (method.getName().equals("getEntities")) return List.of(entity);
                    throw new AssertionError(method.toString());
                });
            var oldState = Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, layers);
            var next = oldState.setValue(SnowLayerBlock.LAYERS, layers + 1);
            Block.pushEntitiesUp(oldState, next, level, new BlockPos(8, 0, 8));
            rows.append(layers).append("\t").append(layers + 1).append("\t").append(oldY + movedY[0]).append("\n");
        }
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), rows.toString());
    }
}
