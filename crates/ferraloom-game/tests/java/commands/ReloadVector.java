import com.google.gson.*;
import com.mojang.brigadier.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.*;
import net.minecraft.server.*;
import net.minecraft.server.commands.ReloadCommand;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.server.permissions.*;
import net.minecraft.world.level.DataPackConfig;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.storage.WorldData;
import static org.mockito.Mockito.*;

/** Invokes the registered command, observing feedback before the asynchronous host request. */
public final class ReloadVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var source=mock(CommandSourceStack.class);
        var server=mock(MinecraftServer.class);
        var packs=mock(PackRepository.class);
        var data=mock(WorldData.class);
        doReturn(server).when(source).getServer();
        doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4))).when(source).permissions();
        doReturn(packs).when(server).getPackRepository();
        doReturn(data).when(server).getWorldData();
        doReturn(WorldDataConfiguration.DEFAULT).when(data).getDataConfiguration();
        doReturn(List.of()).when(packs).getSelectedIds();
        doReturn(List.of()).when(packs).getAvailableIds();
        var events=new JsonArray();
        doAnswer(call->{
            var event=new JsonObject();
            event.add("feedback",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,((Supplier<Component>)call.getArgument(0)).get()).getOrThrow());
            event.addProperty("broadcast",(boolean)call.getArgument(1));events.add(event);return null;
        }).when(source).sendSuccess(any(),anyBoolean());
        doAnswer(call->{var event=new JsonObject();event.addProperty("reload",17);events.add(event);return CompletableFuture.completedFuture(null);}).when(server).reloadResources(anyCollection());
        var dispatcher=new CommandDispatcher<CommandSourceStack>();ReloadCommand.register(dispatcher);
        int value=dispatcher.execute("reload",source);
        var result=new JsonObject();result.addProperty("value",value);result.add("events",events);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
    }
}
