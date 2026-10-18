import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.commands.CommandResultCallback;
import net.minecraft.commands.execution.*;
import net.minecraft.util.profiling.ProfilerFiller;
import static org.mockito.Mockito.*;

/** Runs native queue actions, retaining the precise limit and frame-discard boundaries. */
public final class ExecutionVector {
    static int integer(ExecutionContext<?> context,String name) throws Exception {var field=ExecutionContext.class.getDeclaredField(name);field.setAccessible(true);return field.getInt(context);}
    static Frame top(ExecutionContext<?> context) throws Exception {var method=ExecutionContext.class.getDeclaredMethod("createTopFrame",ExecutionContext.class,CommandResultCallback.class);method.setAccessible(true);return (Frame)method.invoke(null,context,CommandResultCallback.EMPTY);}
    static void add(ExecutionContext<String> context,Frame frame,List<Integer> output,int value) {context.queueNext(new CommandQueueEntry<String>(frame,(next,current)->{output.add(value);next.incrementCost();}));}
    public static void main(String[] args) throws Exception {
        var out=new JsonArray();
        for(String mode:List.of("quota:-1","quota:0","quota:1","quota:2","quota:4","insertion","discard-root","discard-depth","discard-new","reentrant")) {
            int quota=mode.startsWith("quota:")?Integer.parseInt(mode.substring(6)):10;
            var context=new ExecutionContext<String>(quota,10,mock(ProfilerFiller.class));var output=new ArrayList<Integer>();var root=top(context);
            if(mode.startsWith("quota:")) {add(context,root,output,1);add(context,root,output,2);}
            else if(mode.equals("insertion")) {
                context.queueNext(new CommandQueueEntry<String>(root,(next,frame)->{output.add(1);next.incrementCost();add(next,frame,output,2);add(next,frame,output,3);}));add(context,root,output,4);
            } else if(mode.equals("discard-root")||mode.equals("discard-new")) {
                context.queueNext(new CommandQueueEntry<String>(root,(next,frame)->{output.add(1);next.incrementCost();if(mode.equals("discard-new"))add(next,frame,output,2);frame.discard();}));add(context,root,output,9);
            } else if(mode.equals("discard-depth")) {
                var child=new Frame(1,CommandResultCallback.EMPTY,context.frameControlForDepth(1));var grandchild=new Frame(2,CommandResultCallback.EMPTY,context.frameControlForDepth(2));
                context.queueNext(new CommandQueueEntry<String>(child,(next,frame)->{output.add(1);next.incrementCost();frame.discard();}));add(context,grandchild,output,2);add(context,child,output,3);add(context,root,output,4);add(context,grandchild,output,5);
            } else {
                var child=new Frame(2,CommandResultCallback.EMPTY,context.frameControlForDepth(2));context.queueNext(new CommandQueueEntry<String>(child,(next,frame)->{try{output.add(top(next).depth());}catch(Exception e){throw new RuntimeException(e);}next.incrementCost();}));
            }
            context.runCommandQueue();var row=new JsonObject();row.addProperty("mode",mode);row.addProperty("quota",quota);row.add("output",new Gson().toJsonTree(output));row.addProperty("remaining",integer(context,"commandQuota"));row.addProperty("depth",integer(context,"currentFrameDepth"));out.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
