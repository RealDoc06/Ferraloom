import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.structures.*;

/** Child geometry with controlled live water, ice, air and solid block reads. */
public final class MonumentRoomGolden {
    static BlockState initial(int mode,BlockPos p) {
        if(mode==0)return p.getY()<80?Blocks.WATER.defaultBlockState():Blocks.AIR.defaultBlockState();
        return switch(Math.floorMod(p.getX()*3+p.getY()*5+p.getZ(),8)) {
            case 0 -> Blocks.WATER.defaultBlockState();case 1 -> Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,3);
            case 2 -> Blocks.ICE.defaultBlockState();case 3 -> Blocks.PACKED_ICE.defaultBlockState();case 4 -> Blocks.BLUE_ICE.defaultBlockState();case 5 -> Blocks.STONE.defaultBlockState();default -> Blocks.AIR.defaultBlockState();
        };
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        for(long sample=0;sample<8;sample++)for(var direction:Direction.Plane.HORIZONTAL) {
            long seed=sample*341873128712L+132897987541L;
            var building=new OceanMonumentPieces.MonumentBuilding(new LegacyRandomSource(seed),-46,8,direction);
            var children=(List<?>)MonumentGolden.field(building,"childPieces");
            for(int childIndex=0;childIndex<children.size();childIndex++) {
                var child=(StructurePiece)children.get(childIndex);String kind=child.getClass().getSimpleName();
                if(!Set.of("OceanMonumentEntryRoom","OceanMonumentCoreRoom","OceanMonumentSimpleTopRoom").contains(kind))continue;
                var original=child.getBoundingBox();child.getBoundingBox().move(-original.minX(),64-original.minY(),-original.minZ());
                for(int mode=0;mode<2;mode++)for(int clip=0;clip<2;clip++) {
                    int terrain=mode;var changed=new HashMap<BlockPos,BlockState>();var writes=new JsonArray();
                    var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(MonumentRoomGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
                        case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],initial(terrain,(BlockPos)args[0]));
                        case "getFluidState" -> changed.getOrDefault((BlockPos)args[0],initial(terrain,(BlockPos)args[0])).getFluidState();
                        case "getSeaLevel" -> 67;
                        case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);yield true;}
                        case "scheduleTick" -> null;
                        default -> throw new UnsupportedOperationException(method.toString());
                    });
                    var bounds=clip==0?new BoundingBox(0,-64,0,15,319,15):new BoundingBox(4,65,3,14,70,12);var random=new LegacyRandomSource(17);
                    child.postProcess(world,null,null,random,bounds,new ChunkPos(0,0),BlockPos.ZERO);
                    var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("direction",direction.get2DDataValue());item.addProperty("child",childIndex);item.addProperty("kind",kind);item.addProperty("mode",mode);item.add("bounds",StructureGolden.bounds(bounds));item.addProperty("nbt",child.createTag(serialization).toString());item.addProperty("next",random.nextInt());item.add("writes",writes);out.add(item);
                }
            }
        }
        StructureGolden.write(directory.resolve("monument_room_blocks.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
