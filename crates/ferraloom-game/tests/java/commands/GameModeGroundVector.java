import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.*;
import static org.mockito.Mockito.*;

/** Runs the native mode-change ground predicate and real getAvailableSpaceBelow. */
public final class GameModeGroundVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var method=ServerPlayerGameMode.class.getDeclaredMethod("isInRangeOfGround");method.setAccessible(true);
        var levelField=Entity.class.getDeclaredField("level");levelField.setAccessible(true);
        var rows=new JsonArray();
        for(boolean ceiling:List.of(false,true))for(double y:new double[]{63.5,64,64.5,64.9999999,65,65.000001,66,80}) {
            var level=mock(ServerLevel.class);var player=mock(ServerPlayer.class);
            var bb=EntityTypes.PLAYER.getDimensions().makeBoundingBox(0.5,y,0.5);
            doReturn(level).when(player).level();levelField.set(player,level);
            doReturn(bb).when(player).getBoundingBox();doReturn(new WorldBorder()).when(level).getWorldBorder();
            doReturn(List.of()).when(level).getEntityCollisions(any(),any());
            var shapes=new ArrayList<VoxelShape>();shapes.add(Shapes.create(new AABB(0,63,0,1,64,1)));
            if(ceiling)shapes.add(Shapes.create(new AABB(0,65,0,1,66,1)));
            doAnswer(call->{AABB bounds=call.getArgument(1);return shapes.stream().filter(s->s.bounds().intersects(bounds)).toList();}).when(level).getBlockCollisions(any(),any());
            doCallRealMethod().when(player).getAvailableSpaceBelow(anyDouble());
            var row=new JsonObject();row.addProperty("y",y);row.addProperty("ceiling",ceiling);
            row.addProperty("near",(boolean)method.invoke(new ServerPlayerGameMode(player)));rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
    }
}
