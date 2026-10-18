import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.*;
import net.minecraft.server.*;
import net.minecraft.server.commands.TickCommand;
import static org.mockito.Mockito.*;

/** Captures TickCommand's real feedback with controlled timing samples and manager results. */
public final class TickVector {
    record Case(String command, float rate, long average, boolean frozen, boolean stepping, boolean sprinting, boolean outcome) {}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var cases = List.of(
            new Case("tick query", 20f, 12350000, false, false, false, false),
            new Case("tick query", 20f, 60000000, false, false, false, false),
            new Case("tick query", 20f, 12350000, true, false, false, false),
            new Case("tick query", 20f, 12350000, true, false, true, false),
            new Case("tick rate 20.05", 20f, 0, false, false, false, false),
            new Case("tick rate 20.15", 20f, 0, false, false, false, false),
            new Case("tick rate 10000", 20f, 0, false, false, false, false),
            new Case("tick freeze", 20f, 0, false, true, true, false),
            new Case("tick unfreeze", 20f, 0, true, false, false, false),
            new Case("tick step", 20f, 0, false, false, false, false),
            new Case("tick step 30", 20f, 0, true, false, false, true),
            new Case("tick step stop", 20f, 0, true, false, false, false),
            new Case("tick step stop", 20f, 0, true, true, false, true),
            new Case("tick sprint 30", 20f, 0, false, false, false, false),
            new Case("tick sprint 30", 20f, 0, false, false, true, true),
            new Case("tick sprint stop", 20f, 0, false, false, false, false),
            new Case("tick sprint stop", 20f, 0, false, false, true, true)
        );
        var vectors = new JsonArray();
        for (var test : cases) {
            var vector = new Gson().toJsonTree(test).getAsJsonObject();
            var source = mock(CommandSourceStack.class); var server = mock(MinecraftServer.class);
            var manager = mock(ServerTickRateManager.class);
            doReturn(server).when(source).getServer(); doReturn(manager).when(server).tickRateManager();
            doReturn(test.rate).when(manager).tickrate(); doReturn(50000000L).when(manager).nanosecondsPerTick();
            doReturn(test.average).when(server).getAverageTickTimeNanos();
            var samples = new long[100]; for (int i=0;i<100;i++) samples[i]=(99-i)*123456L;
            doReturn(samples).when(server).getTickTimesNanos();
            doReturn(test.frozen).when(manager).isFrozen(); doReturn(test.stepping).when(manager).isSteppingForward();
            doReturn(test.sprinting).when(manager).isSprinting();
            doReturn(test.outcome).when(manager).stepGameIfPaused(anyInt()); doReturn(test.outcome).when(manager).stopStepping();
            doReturn(test.outcome).when(manager).requestGameToSprint(anyInt()); doReturn(test.outcome).when(manager).stopSprinting();
            var feedback = new JsonArray(); vector.add("feedback", feedback);
            doAnswer(invocation -> {
                var entry = new JsonObject(); entry.addProperty("broadcast", (boolean)invocation.getArgument(1));
                entry.add("message", ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, ((Supplier<Component>)invocation.getArgument(0)).get()).getOrThrow());
                feedback.add(entry); return null;
            }).when(source).sendSuccess(any(),anyBoolean());
            var words = test.command.split(" ");
            String method; Class<?> valueType=null; Object value=null;
            switch (words[1]) {
                case "query" -> method="tickQuery";
                case "rate" -> { method="setTickingRate"; valueType=float.class; value=Float.parseFloat(words[2]); }
                case "freeze", "unfreeze" -> { method="setFreeze"; valueType=boolean.class; value=words[1].equals("freeze"); }
                case "step" -> {
                    method=words.length>2 && words[2].equals("stop")?"stopStepping":"step";
                    if (method.equals("step")) { valueType=int.class; value=words.length==2?1:Integer.parseInt(words[2]); }
                }
                case "sprint" -> {
                    method=words[2].equals("stop")?"stopSprinting":"sprint";
                    if (method.equals("sprint")) { valueType=int.class; value=Integer.parseInt(words[2]); }
                }
                default -> throw new AssertionError();
            }
            var target = valueType==null ? TickCommand.class.getDeclaredMethod(method,CommandSourceStack.class) : TickCommand.class.getDeclaredMethod(method,CommandSourceStack.class,valueType);
            target.setAccessible(true);
            try { vector.addProperty("value", (int)(valueType==null ? target.invoke(null,source) : target.invoke(null,source,value))); }
            catch (java.lang.reflect.InvocationTargetException failure) {
                var error=(com.mojang.brigadier.exceptions.CommandSyntaxException)failure.getCause(); vector.addProperty("error", error.getRawMessage().getString());
            }
            vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
