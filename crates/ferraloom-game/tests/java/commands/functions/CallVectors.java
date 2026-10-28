import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.commands.*;
import net.minecraft.commands.execution.*;
import net.minecraft.commands.execution.tasks.CallFunction;
import net.minecraft.commands.functions.InstantiatedFunction;
import net.minecraft.resources.Identifier;
import net.minecraft.util.profiling.ProfilerFiller;
import static org.mockito.Mockito.*;

/** Observe the actual CallFunction, ContinuationTask, and Frame discard controls. */
public final class CallVectors {
    public static void main(String[] args) throws Exception {
        var output = new JsonArray();
        var bodies = List.of(List.<String>of(), List.of("seed"), List.of("seed", "seed"),
            List.of("seed", "seed", "seed"), List.of("return 7", "seed", "seed"),
            List.of("seed", "return fail", "seed"), List.of("seed", "seed", "return -4", "seed"));
        for(var body : bodies) for(boolean parent : List.of(false, true)) for(int quota : List.of(1,2,10)) {
            var events = new ArrayList<String>();
            var context = new ExecutionContext<CommandSourceStack>(quota,10,mock(ProfilerFiller.class));
            var root = new Frame(0, CommandResultCallback.EMPTY, () -> context.discardAtDepthOrHigher(0));
            CommandResultCallback callback = (success,value) -> events.add("result:"+success+":"+value);
            var actions = new ArrayList<UnboundEntryAction<CommandSourceStack>>();
            for(String command : body) actions.add((sender,next,frame) -> {
                if(command.equals("seed")) { next.incrementCost(); events.add("seed:"+frame.depth()); }
                else { if(command.equals("return fail")) frame.returnFailure();
                    else frame.returnSuccess(Integer.parseInt(command.substring(7))); frame.discard(); }
            });
            var function = new InstantiatedFunction<CommandSourceStack>() {
                public Identifier id() { return Identifier.parse("test:call"); }
                public List<UnboundEntryAction<CommandSourceStack>> entries() { return actions; }
            };
            var task = new CallFunction<CommandSourceStack>(function,callback,parent);
            context.queueNext(new CommandQueueEntry<>(root,task.bind(mock(CommandSourceStack.class))));
            context.queueNext(new CommandQueueEntry<>(root,(next,frame)-> {events.add("tail");next.incrementCost();}));
            context.runCommandQueue();
            var quotaField=ExecutionContext.class.getDeclaredField("commandQuota");quotaField.setAccessible(true);
            var row=new JsonObject();row.add("body",new Gson().toJsonTree(body));row.addProperty("parent",parent);
            row.addProperty("quota",quota);row.add("events",new Gson().toJsonTree(events));
            row.addProperty("remaining",quotaField.getInt(context));output.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(output)+"\n");
        System.out.println("Captured "+output.size()+" native function frame cases");
    }
}
