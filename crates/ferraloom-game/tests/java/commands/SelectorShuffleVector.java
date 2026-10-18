import com.google.gson.*;
import java.nio.file.*;
import java.util.*;

/** Checks java.util.Collections' shuffle algorithm and retained Random draw consumption. */
public final class SelectorShuffleVector {
    public static void main(String[] args) throws Exception {
        var gson=new GsonBuilder().setPrettyPrinting().create();var vectors=new JsonArray();
        for(long seed:new long[]{0L,1L,-1L,Long.MIN_VALUE,Long.MAX_VALUE}) for(int size:new int[]{0,1,2,3,8,17,32}) {
            var random=new Random(seed);var values=new ArrayList<Integer>();for(int i=0;i<size;i++)values.add(i);
            var vector=new JsonObject();vector.addProperty("seed",seed);vector.addProperty("size",size);
            Collections.shuffle(values,random);vector.add("first",gson.toJsonTree(values));
            Collections.shuffle(values,random);vector.add("second",gson.toJsonTree(values));
            vector.addProperty("next_long",random.nextLong());vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]),gson.toJson(vectors)+"\n");
    }
}
