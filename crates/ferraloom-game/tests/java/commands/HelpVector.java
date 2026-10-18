import com.google.gson.*;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.permissions.*;
import net.minecraft.world.flag.FeatureFlags;
import static org.mockito.Mockito.*;

/** Runs vanilla's registered HelpCommand against its complete dedicated command tree. */
public final class HelpVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        CommandSyntaxException.BUILT_IN_EXCEPTIONS=new BrigadierExceptions();
        var registries=VanillaRegistries.createWorldLookup();
        var commands=new Commands(Commands.CommandSelection.DEDICATED,CommandBuildContext.simple(registries,FeatureFlags.DEFAULT_FLAGS));
        var dispatcher=commands.getDispatcher();var out=new JsonArray();
        for(int level:List.of(0,4)) for(String input:List.of("help","help seed","help tp","help tp @a","help say","help time","help time set","help return run","help does_not_exist","help /seed")) {
            if(level==4 && input.equals("help"))continue;
            var source=mock(CommandSourceStack.class);when(source.callback()).thenReturn(CommandResultCallback.EMPTY);when(source.permissions()).thenReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(level)));
            var feedback=new JsonArray();doAnswer(call->{var event=new JsonObject();event.add("message",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,((Supplier<Component>)call.getArgument(0)).get()).getOrThrow());event.addProperty("broadcast",(boolean)call.getArgument(1));feedback.add(event);return null;}).when(source).sendSuccess(any(),anyBoolean());
            var row=new JsonObject();row.addProperty("input",input);row.addProperty("level",level);
            try {row.addProperty("value",dispatcher.execute(input,source));} catch(CommandSyntaxException e) {row.add("error",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,ComponentUtils.fromMessage(e.getRawMessage())).getOrThrow());}
            row.add("feedback",feedback);out.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
