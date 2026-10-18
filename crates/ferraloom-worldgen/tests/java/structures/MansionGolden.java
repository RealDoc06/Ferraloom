import com.google.gson.*;
import java.nio.file.*;
import java.lang.reflect.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.LegacyRandomSource;

/** Observe the real mansion grid planner, including private floor-room metadata. */
public final class MansionGolden {
    static Object field(Object o,String name)throws Exception {var f=o.getClass().getDeclaredField(name);f.setAccessible(true);return f.get(o);}
    static JsonArray cells(Object grid)throws Exception {
        int[][] values=(int[][])field(grid,"grid");var out=new JsonArray();
        for(int y=0;y<11;y++)for(int x=0;x<11;x++)out.add(values[x][y]);return out;
    }
    static void grids(Path directory)throws Exception {
        var type=Class.forName("net.minecraft.world.level.levelgen.structure.structures.WoodlandMansionPieces$MansionGrid");
        var constructor=type.getDeclaredConstructor(RandomSource.class);constructor.setAccessible(true);
        var out=new JsonArray();
        for(long sample=0;sample<256;sample++) {
            long seed=sample*341873128712L+132897987541L;var random=new LegacyRandomSource(seed);var grid=constructor.newInstance(random);
            var item=new JsonObject();item.addProperty("seed",seed);item.add("base",cells(field(grid,"baseGrid")));item.add("third",cells(field(grid,"thirdFloorGrid")));
            Object[] rooms=(Object[])field(grid,"floorRooms");for(int i=0;i<3;i++)item.add("rooms"+i,cells(rooms[i]));item.addProperty("next",random.nextInt());out.add(item);
        }
        StructureGolden.write(directory.resolve("mansion_grids.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {grids(Path.of(args[0]));}
}
