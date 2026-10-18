import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import com.mojang.brigadier.StringReader;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.coordinates.*;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.SetWorldSpawnCommand;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.LevelData;
import net.minecraft.world.phys.Vec2;
import static org.mockito.Mockito.*;

/** Invokes the real setworldspawn command and RespawnData codec. */
public final class SpawnVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var method = SetWorldSpawnCommand.class.getDeclaredMethod("setSpawn", CommandSourceStack.class, BlockPos.class, Coordinates.class);
        method.setAccessible(true);
        var vectors = new JsonArray();
        for (var input : List.of("450 100", "~30 ~-20", "-180 -90")) {
            var vector = new JsonObject(); vector.addProperty("rotation", input);
            var source = mock(CommandSourceStack.class);
            var level = mock(ServerLevel.class);
            doReturn(level).when(source).getLevel();
            doReturn(Level.OVERWORLD).when(level).dimension();
            doReturn(new Vec2(-60,35)).when(source).getRotation();
            doAnswer(invocation -> {
                LevelData.RespawnData data = invocation.getArgument(0);
                vector.addProperty("nbt", LevelData.RespawnData.CODEC.encodeStart(NbtOps.INSTANCE, data).getOrThrow().toString());
                return null;
            }).when(level).setRespawnData(any());
            doAnswer(invocation -> {
                Component component = ((Supplier<Component>)invocation.getArgument(0)).get();
                vector.add("component", ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow());
                return null;
            }).when(source).sendSuccess(any(), eq(true));
            method.invoke(null, source, new BlockPos(1,80,-2), RotationArgument.rotation().parse(new StringReader(input)));
            vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(vectors) + "\n");
    }
}
