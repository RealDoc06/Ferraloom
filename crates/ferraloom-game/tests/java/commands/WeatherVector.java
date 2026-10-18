import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.*;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.commands.WeatherCommand;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import static org.mockito.Mockito.*;

/** Native command return values, state parameters and subsequent RNG position. */
public final class WeatherVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var vectors = new JsonArray();
        for (long seed : new long[]{0,1,-1}) for (String action : List.of("clear","rain","thunder")) for (int duration : new int[]{-1,1,1200,Integer.MAX_VALUE}) {
            var vector = new JsonObject(); vector.addProperty("seed",seed);vector.addProperty("action",action);vector.addProperty("duration",duration);
            var source=mock(CommandSourceStack.class);var server=mock(MinecraftServer.class);var level=mock(ServerLevel.class);
            var random=new LegacyRandomSource(seed);
            doReturn(server).when(source).getServer();doReturn(level).when(source).getLevel();doReturn(random).when(level).getRandom();
            doAnswer(call->{vector.add("parameters",new Gson().toJsonTree(List.of(call.getArgument(0),call.getArgument(1),call.getArgument(2),call.getArgument(3))));return null;}).when(server).setWeatherParameters(anyInt(),anyInt(),anyBoolean(),anyBoolean());
            doAnswer(call->{vector.addProperty("broadcast",(boolean)call.getArgument(1));vector.add("feedback",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,((Supplier<Component>)call.getArgument(0)).get()).getOrThrow());return null;}).when(source).sendSuccess(any(),anyBoolean());
            var name="set"+Character.toUpperCase(action.charAt(0))+action.substring(1);
            var method=WeatherCommand.class.getDeclaredMethod(name,CommandSourceStack.class,int.class);method.setAccessible(true);
            vector.addProperty("value",(int)method.invoke(null,source,duration));vector.addProperty("next_random",random.nextLong());vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
