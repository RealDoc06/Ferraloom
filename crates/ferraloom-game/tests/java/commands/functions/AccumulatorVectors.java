import com.google.gson.*;
import java.nio.file.*;
import java.util.*;

/** Observe FunctionCommand's native local accumulator, including signed overflow. */
public final class AccumulatorVectors {
    public static void main(String[] args) throws Exception {
        var type=Class.forName("net.minecraft.server.commands.FunctionCommand$1Accumulator");
        var constructor=type.getDeclaredConstructor();constructor.setAccessible(true);
        var add=type.getDeclaredMethod("add",int.class);add.setAccessible(true);
        var any=type.getDeclaredField("anyResult");any.setAccessible(true);
        var sum=type.getDeclaredField("sum");sum.setAccessible(true);
        var rows=new JsonArray();
        for(var values:List.of(List.<Integer>of(),List.of(0),List.of(7),List.of(-4,7),
                List.of(Integer.MAX_VALUE,1),List.of(Integer.MIN_VALUE,-1),List.of(Integer.MAX_VALUE,Integer.MAX_VALUE,2))) {
            var accumulator=constructor.newInstance();
            for(int value:values) add.invoke(accumulator,value);
            var row=new JsonObject();row.add("values",new Gson().toJsonTree(values));
            row.addProperty("any",any.getBoolean(accumulator));row.addProperty("sum",sum.getInt(accumulator));rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
        System.out.println("Captured "+rows.size()+" native function accumulators");
    }
}
