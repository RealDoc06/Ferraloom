import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.ForceLoadCommand;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;
import static org.mockito.Mockito.*;

/** Executes ForceLoadCommand's real methods against a canonical mutable forced-ticket set. */
public final class ForceLoadVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var change = ForceLoadCommand.class.getDeclaredMethod("changeForceLoad", CommandSourceStack.class, ColumnPos.class, ColumnPos.class, boolean.class);
        var query = ForceLoadCommand.class.getDeclaredMethod("queryForceLoad", CommandSourceStack.class, ColumnPos.class);
        var list = ForceLoadCommand.class.getDeclaredMethod("listForceLoad", CommandSourceStack.class);
        var remove = ForceLoadCommand.class.getDeclaredMethod("removeAll", CommandSourceStack.class);
        for (var method : List.of(change,query,list,remove)) method.setAccessible(true);
        var level = mock(ServerLevel.class); var source = mock(CommandSourceStack.class);
        var chunks = new it.unimi.dsi.fastutil.longs.LongOpenHashSet();
        doReturn(level).when(source).getLevel(); doReturn(Level.OVERWORLD).when(level).dimension(); doReturn(chunks).when(level).getForceLoadedChunks();
        doAnswer(invocation -> {
            long chunk = ChunkPos.pack(invocation.getArgument(0), invocation.getArgument(1));
            return (boolean)invocation.getArgument(2) ? chunks.add(chunk) : chunks.remove(chunk);
        }).when(level).setChunkForced(anyInt(),anyInt(),anyBoolean());
        var results = new JsonArray();
        for (var command : List.of("forceload query", "forceload add 0 0 16 0", "forceload query", "forceload add 0 0 16 0", "forceload remove 16 0 32 0", "forceload query 16 0", "forceload add -1 -1", "forceload query", "forceload remove all", "forceload remove all", "forceload add -30000001 0", "forceload add 0 0 4096 0")) {
            var vector = new JsonObject(); vector.addProperty("command", command);
            var feedback = new JsonArray(); vector.add("feedback", feedback);
            doAnswer(invocation -> {
                var entry = new JsonObject(); entry.addProperty("failure", false); entry.addProperty("broadcast", (boolean)invocation.getArgument(1));
                entry.add("message", ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, ((Supplier<Component>)invocation.getArgument(0)).get()).getOrThrow()); feedback.add(entry); return null;
            }).when(source).sendSuccess(any(),anyBoolean());
            doAnswer(invocation -> {
                var entry = new JsonObject(); entry.addProperty("failure", true); entry.addProperty("broadcast", false);
                entry.add("message", ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, (Component)invocation.getArgument(0)).getOrThrow()); feedback.add(entry); return null;
            }).when(source).sendFailure(any());
            var words = command.split(" ");
            try {
                Object value;
                if (words[1].equals("query")) value = words.length == 2 ? list.invoke(null,source) : query.invoke(null,source,new ColumnPos(Integer.parseInt(words[2]),Integer.parseInt(words[3])));
                else if (words[2].equals("all")) value = remove.invoke(null,source);
                else {
                    var from = new ColumnPos(Integer.parseInt(words[2]),Integer.parseInt(words[3]));
                    var to = words.length == 4 ? from : new ColumnPos(Integer.parseInt(words[4]),Integer.parseInt(words[5]));
                    value = change.invoke(null,source,from,to,words[1].equals("add"));
                }
                vector.addProperty("value", (int)value);
            } catch (java.lang.reflect.InvocationTargetException failure) {
                var error = (com.mojang.brigadier.exceptions.CommandSyntaxException)failure.getCause(); vector.addProperty("error", error.getRawMessage().getString());
            }
            var forced = new JsonArray(); chunks.longStream().sorted().forEach(forced::add); vector.add("forced", forced); results.add(vector);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(results) + "\n");
    }
}
