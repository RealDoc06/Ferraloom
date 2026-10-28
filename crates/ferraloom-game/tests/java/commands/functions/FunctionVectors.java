import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.execution.*;
import net.minecraft.commands.functions.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.FunctionCommand;
import net.minecraft.util.profiling.ProfilerFiller;
import static org.mockito.Mockito.*;

/** Run FunctionCommand's real callback decoration, accumulator and return chain. */
public final class FunctionVectors {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var rows=new JsonArray();
        var cases=List.of(List.of(List.of("seed")),List.of(List.of("return 7")),List.of(List.of("return fail")),
            List.of(List.<String>of(),List.of("return 7")),List.of(List.of("return fail"),List.of("return 7")),
            List.of(List.of("return 2147483647"),List.of("return 1")),List.of(List.<String>of(),List.<String>of()),
            List.of(List.of("seed","return -4","seed"),List.of("return 9")));
        for(var bodies:cases) for(boolean silent:List.of(false,true)) for(boolean hasCallback:List.of(false,true))
        for(boolean returning:List.of(false,true)) for(int quota:List.of(1,20)) {
            var events=new ArrayList<String>();
            var context=new ExecutionContext<CommandSourceStack>(quota,10,mock(ProfilerFiller.class));
            CommandResultCallback frameCallback=(success,value)->events.add("frame:"+success+":"+value);
            var root=new Frame(0,frameCallback,()->context.discardAtDepthOrHigher(0));
            var original=mock(CommandSourceStack.class);var functionSource=mock(CommandSourceStack.class);
            when(original.isSilent()).thenReturn(silent);
            CommandResultCallback callback=hasCallback?(success,value)->events.add("original:"+success+":"+value):CommandResultCallback.EMPTY;
            when(original.callback()).thenReturn(callback);when(original.dispatcher()).thenReturn(new CommandDispatcher<>());
            when(functionSource.clearCallbacks()).thenReturn(functionSource);
            var functions=new ArrayList<CommandFunction<CommandSourceStack>>();
            for(int i=0;i<bodies.size();i++) {
                var actions=new ArrayList<UnboundEntryAction<CommandSourceStack>>();
                for(String command:bodies.get(i)) actions.add((sender,next,frame)-> {
                    if(command.equals("seed")) {next.incrementCost();events.add("seed:"+frame.depth());}
                    else {if(command.equals("return fail"))frame.returnFailure();else frame.returnSuccess(Integer.parseInt(command.substring(7)));frame.discard();}
                });
                functions.add(new PlainTextFunction<>(Identifier.parse("test:f"+i),actions));
            }
            FunctionCommand.queueFunctions(functions,null,original,functionSource,ExecutionControl.create(context,root),
                (sender,id,value)->events.add(Component.translatable("commands.function.result",Component.translationArg(id),value).getString()),
                returning?ChainModifiers.DEFAULT.setReturn():ChainModifiers.DEFAULT);
            context.queueNext(new CommandQueueEntry<>(root,(next,frame)->{events.add("tail");next.incrementCost();}));
            context.runCommandQueue();
            var quotaField=ExecutionContext.class.getDeclaredField("commandQuota");quotaField.setAccessible(true);
            var row=new JsonObject();row.add("bodies",new Gson().toJsonTree(bodies));row.addProperty("silent",silent);
            row.addProperty("callback",hasCallback);row.addProperty("returning",returning);row.addProperty("quota",quota);
            row.add("events",new Gson().toJsonTree(events));row.addProperty("remaining",quotaField.getInt(context));rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
        System.out.println("Captured "+rows.size()+" native FunctionCommand cases");
    }
}
