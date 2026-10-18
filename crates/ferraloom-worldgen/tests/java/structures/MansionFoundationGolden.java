import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;

/** Mansion afterPlace uses raw piece footprints and live non-air/liquid tests. */
public final class MansionFoundationGolden {
    static BlockState initial(int mode,BlockPos p) {
        if(p.getY()==50)return Math.floorMod(p.getX()*3+p.getZ(),5)==0?Blocks.AIR.defaultBlockState():Blocks.DARK_OAK_PLANKS.defaultBlockState();
        if(mode==2)return Blocks.AIR.defaultBlockState();
        if(mode==3&&p.getY()==44)return Blocks.OAK_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true);
        int ground=40+Math.floorMod(p.getX()+p.getZ()*3,6);
        return p.getY()<=ground?Blocks.STONE.defaultBlockState():mode==1?Blocks.WATER.defaultBlockState():Blocks.AIR.defaultBlockState();
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var structure=registries.lookupOrThrow(Registries.STRUCTURE).getValue(Identifier.withDefaultNamespace("mansion"));
        var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        for(int mode=0;mode<4;mode++)for(int clip=0;clip<4;clip++) {
            int terrain=mode;var pieces=new ArrayList<StructurePiece>();var tags=new JsonArray();
            for(int[] box:new int[][]{{-2,50,-3,5,56,7},{8,53,4,19,61,12},{10,50,0,15,58,5}}) {
                var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{id:'minecraft:btp',GD:0,O:-1,BB:[I;"+Arrays.stream(box).mapToObj(Integer::toString).collect(java.util.stream.Collectors.joining(","))+"]}");
                var piece=new net.minecraft.world.level.levelgen.structure.structures.BuriedTreasurePieces.BuriedTreasurePiece(tag);pieces.add(piece);tags.add(tag.toString());
            }
            var writes=new JsonArray();var changed=new HashMap<BlockPos,BlockState>();
            var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(MansionFoundationGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
                case "getMinY" -> -64;
                case "isEmptyBlock" -> changed.getOrDefault((BlockPos)args[0],initial(terrain,(BlockPos)args[0])).isAir();
                case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],initial(terrain,(BlockPos)args[0]));
                case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));b.addProperty("flags",(Integer)args[2]);writes.add(b);yield true;}
                default -> throw new UnsupportedOperationException(method.toString());
            });
            var bounds=new BoundingBox((clip&1)*8,70,((clip>>1)&1)*8,(clip&1)*8+7,90,((clip>>1)&1)*8+7);
            structure.afterPlace(world,null,null,new LegacyRandomSource(17),bounds,new ChunkPos(0,0),new PiecesContainer(pieces));
            var item=new JsonObject();item.addProperty("mode",mode);item.add("pieces",tags);item.add("bounds",StructureGolden.bounds(bounds));item.add("writes",writes);out.add(item);
        }
        StructureGolden.write(directory.resolve("mansion_foundations.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
