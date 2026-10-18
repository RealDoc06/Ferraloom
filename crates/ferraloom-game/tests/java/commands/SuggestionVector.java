import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import com.mojang.brigadier.*;
import com.mojang.brigadier.context.*;
import com.mojang.brigadier.suggestion.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.*;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.TickCommand;
import net.minecraft.server.permissions.*;
import static org.mockito.Mockito.*;

/** Captures cursor ranges and translated tooltips, as well as completion text. */
public final class SuggestionVector {
    static JsonObject capture(String kind,String input,int level,Suggestions suggestions) {
        var vector = new JsonObject();vector.addProperty("kind",kind);vector.addProperty("input",input);vector.addProperty("level",level);
        vector.addProperty("start",suggestions.getRange().getStart());vector.addProperty("end",suggestions.getRange().getEnd());
        var entries = new JsonArray();
        suggestions.getList().forEach(s -> {
            var entry = new JsonObject();entry.addProperty("text",s.getText());
            if (s.getTooltip() instanceof Component tooltip) entry.add("tooltip",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,tooltip).getOrThrow());
            entries.add(entry);
        });
        vector.add("suggestions",entries);return vector;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var vectors = new JsonArray();
        var dispatcher = new CommandDispatcher<CommandSourceStack>();TickCommand.register(dispatcher);
        for (int level : new int[]{0,4}) {
            var source=mock(CommandSourceStack.class);
            doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(level))).when(source).permissions();
            doReturn(List.of("Alex","Alice","Doc")).when(source).getOnlinePlayerNames();
            doReturn(List.of()).when(source).getSelectedEntities();
            for (var input : List.of("","A","Alex","@","@z","@p","@p ","@pfoo","@a","@n","@a[","@a[lim","@a[limit","@a[limit=","@a[limit=1","@a[limit=1,","@s[","@a[name=Doc,","@a[name=!Doc,","@a[sort=","@a[sort=n","@a[gamemode=","@a[gamemode=!s","@a[gamemode=!creative,gamemode=","@e[type=minecraft:pl","@e[type=","@e[type=!minecraft:player,type=","@a[x=1 ,","@a[x=1!]","@a[]")) {
                var context = new CommandContextBuilder<>(dispatcher,source,dispatcher.getRoot(),0).build(input);
                vectors.add(capture("entity",input,level,EntityArgument.entities().listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
                vectors.add(capture("game_profile",input,level,GameProfileArgument.gameProfile().listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
            }
            if (level==4) for (var input : List.of("tick ","tick rate ","tick rate 2","tick step ","tick step 1","tick sprint ","tick sprint 1")) {
                vectors.add(capture("command",input,level,dispatcher.getCompletionSuggestions(dispatcher.parse(input,source)).join()));
            }
        }
        var resources = List.of(net.minecraft.resources.Identifier.parse("example:thing"),net.minecraft.resources.Identifier.parse("minecraft:thing"),net.minecraft.resources.Identifier.parse("example:other"));
        for (var input : List.of("","!","#","!#","!#th","thing","example:")) {
            vectors.add(capture("resource_prefix",input,4,net.minecraft.commands.SharedSuggestionProvider.suggestResource(resources,new SuggestionsBuilder(input,0),"!#").join()));
            vectors.add(capture("resource_plain",input,4,net.minecraft.commands.SharedSuggestionProvider.suggestResource(resources,new SuggestionsBuilder(input,0)).join()));
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
