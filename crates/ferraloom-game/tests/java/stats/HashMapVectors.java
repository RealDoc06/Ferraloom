import java.lang.reflect.Field;
import java.nio.file.*;
import java.util.*;

/** JDK 25 HashMap.putAll capacity and entry traversal observations. */
public final class HashMapVectors {
    static int key(int i) { return (i * 0x9e3779b9) ^ (i << 12); }
    public static void main(String[] args) throws Exception {
        Field table = HashMap.class.getDeclaredField("table"); table.setAccessible(true);
        var output = new StringBuilder();
        for (int mode = 0; mode < 8; mode++) for (int size : new int[]{0,1,12,13,24,25,48,49,96,97}) {
            var target = switch(mode) {
                case 0,4,5 -> new HashMap<Integer,Integer>();
                case 1 -> new HashMap<Integer,Integer>(1);
                case 2 -> new HashMap<Integer,Integer>(16);
                case 3 -> new HashMap<Integer,Integer>(64);
                case 6 -> new HashMap<Integer,Integer>(1,0.5f);
                default -> new HashMap<Integer,Integer>(1,1.0f);
            };
            int initial = mode == 4 ? 7 : mode == 5 ? 13 : 0;
            for (int i=0;i<initial;i++) target.put(key(i),-i);
            var source = new LinkedHashMap<Integer,Integer>();
            for (int i=size-1;i>=0;i--) source.put(key(i),i+100);
            target.putAll(source);
            Object[] bins=(Object[])table.get(target);
            output.append(mode).append('\t').append(size).append('\t').append(bins==null?0:bins.length).append('\t');
            var entries = new StringJoiner(",");
            target.forEach((k,v)->entries.add(k+":"+v));
            output.append(entries).append('\n');
        }
        Files.writeString(Path.of(args[0]),output);
    }
}
