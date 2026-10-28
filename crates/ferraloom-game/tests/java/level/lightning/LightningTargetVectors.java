import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.village.poi.PoiManager;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import static org.mockito.Mockito.*;

/** Runs actual findLightningTargetAround with a proven empty POI result. */
public final class LightningTargetVectors {
    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        StringBuilder rows = new StringBuilder();
        for (long seed : new long[]{0, 1, 42, 4096, 12345}) for (int mode = 0; mode < 3; mode++) for (int y : new int[]{-17, 0}) {
            ServerLevel level = mock(ServerLevel.class, call -> call.getMethod().getName().equals("findLightningTargetAround") ? call.callRealMethod() : RETURNS_DEFAULTS.answer(call));
            var center = new BlockPos(8, y, 8);
            when(level.getHeightmapPos(Heightmap.Types.MOTION_BLOCKING, center)).thenReturn(center);
            when(level.getMinY()).thenReturn(-16); when(level.getMaxY()).thenReturn(15);
            PoiManager poi = mock(PoiManager.class);
            when(level.getPoiManager()).thenReturn(poi);
            when(poi.findClosest(any(), any(), any(), eq(128), eq(PoiManager.Occupancy.ANY))).thenReturn(Optional.empty());
            var entities = new ArrayList<LivingEntity>();
            for (int index = 0; index < 4; index++) {
                LivingEntity entity = mock(LivingEntity.class);
                BlockPos pos = new BlockPos(7 + index, 1 + index, 8);
                when(entity.blockPosition()).thenReturn(pos);
                when(entity.isAlive()).thenReturn(index != 1 && mode != 0);
                when(level.canSeeSky(pos)).thenReturn(index != 2 && (mode != 2 || index == 3));
                when(entity.getBoundingBox()).thenReturn(new AABB(pos.getX()+0.2,pos.getY(),pos.getZ()+0.2,pos.getX()+0.8,pos.getY()+1,pos.getZ()+0.8));
                entities.add(entity);
            }
            doAnswer(call -> {
                AABB box = call.getArgument(1); Predicate<LivingEntity> filter = call.getArgument(2);
                return entities.stream().filter(e -> e.getBoundingBox().intersects(box)).filter(filter).toList();
            }).when(level).getEntitiesOfClass(eq(LivingEntity.class), any(AABB.class), any(Predicate.class));
            var random = RandomSource.create(seed);
            var field = Level.class.getDeclaredField("random"); field.setAccessible(true); field.set(level, random);
            var method = ServerLevel.class.getDeclaredMethod("findLightningTargetAround", BlockPos.class);
            method.setAccessible(true);
            var target = (BlockPos)method.invoke(level, center);
            rows.append(seed).append('\t').append(mode).append('\t').append(y).append('\t')
                .append(target.getX()).append(',').append(target.getY()).append(',').append(target.getZ()).append('\t')
                .append(random.nextInt()).append('\n');
        }
        Files.writeString(Path.of(args[0]), rows.toString());
    }
}
