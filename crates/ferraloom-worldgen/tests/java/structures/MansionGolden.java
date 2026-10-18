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
    static void pieces(net.minecraft.core.RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("mansion-world")).createAccess("test")) {
            var templates=new net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),net.minecraft.core.registries.BuiltInRegistries.BLOCK);
            var serialization=new net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            for(long sample=0;sample<32;sample++)for(var rotation:net.minecraft.world.level.block.Rotation.values()) {
                long seed=sample*341873128712L+132897987541L;var random=new LegacyRandomSource(seed);
                var pieces=new java.util.ArrayList<net.minecraft.world.level.levelgen.structure.structures.WoodlandMansionPieces.WoodlandMansionPiece>();
                net.minecraft.world.level.levelgen.structure.structures.WoodlandMansionPieces.generateMansion(templates,new net.minecraft.core.BlockPos(-17,70,31),rotation,pieces,random);
                var tags=new net.minecraft.nbt.ListTag();for(var piece:pieces)tags.add(piece.createTag(serialization));
                var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("rotation",rotation.ordinal());item.addProperty("nbt",tags.toString());item.addProperty("next",random.nextInt());out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("mansion_pieces.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();var directory=Path.of(args[0]);grids(directory);pieces(StructureGolden.load(),directory);}
}
