import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.structures.*;

/** Real monument shell and ordered child placement in chunks without elder spawns. */
public final class MonumentBuildingGolden {
    static BlockState initial(int mode,BlockPos p) {
        int ground=mode==0?35:32+Math.floorMod(p.getX()*3+p.getZ()*5,8);
        if(p.getY()<=ground)return Blocks.STONE.defaultBlockState();
        if(mode==1&&Math.floorMod(p.getX()+p.getZ()*3+p.getY(),31)==0)return Blocks.BLUE_ICE.defaultBlockState();
        return p.getY()<=63?Blocks.WATER.defaultBlockState():Blocks.AIR.defaultBlockState();
    }
    static void capture(Path directory)throws Exception {
        var out=new JsonArray();var worldPos=StructurePiece.class.getDeclaredMethod("getWorldPos",int.class,int.class,int.class);worldPos.setAccessible(true);
        for(long seed:new long[]{0,18})for(var direction:Direction.Plane.HORIZONTAL)for(int mode=0;mode<2;mode++) {
            var building=new OceanMonumentPieces.MonumentBuilding(new LegacyRandomSource(seed),-46,8,direction);var elders=new ArrayList<BlockPos>();
            for(Object child:(List<?>)MonumentGolden.field(building,"childPieces")) {
                String kind=child.getClass().getSimpleName();int[] p=null;
                if(kind.equals("OceanMonumentWingRoom"))p=(Integer)MonumentGolden.field(child,"mainDesign")==0?new int[]{11,2,16}:new int[]{11,5,13};
                else if(kind.equals("OceanMonumentPenthouse"))p=new int[]{6,1,6};
                if(p!=null)elders.add(((BlockPos)worldPos.invoke(child,p[0],p[1],p[2])).immutable());
            }
            for(int cx=-3;cx<=0;cx++)for(int cz=0;cz<=4;cz++) {
                var center=new ChunkPos(cx,cz);var bounds=new BoundingBox(center.getMinBlockX(),-64,center.getMinBlockZ(),center.getMaxBlockX(),319,center.getMaxBlockZ());
                if(elders.stream().anyMatch(bounds::isInside))continue;
                int terrain=mode;var writes=new JsonArray();var changed=new HashMap<BlockPos,BlockState>();
                var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(MonumentBuildingGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
                    case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],initial(terrain,(BlockPos)args[0]));
                    case "getFluidState" -> changed.getOrDefault((BlockPos)args[0],initial(terrain,(BlockPos)args[0])).getFluidState();
                    case "getSeaLevel" -> 63;case "getMinY" -> -64;
                    case "setBlock" -> {if((Integer)args[2]!=2)throw new AssertionError("flags");var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);writes.add(StructureGolden.ints(p.getX(),p.getY(),p.getZ(),Block.BLOCK_STATE_REGISTRY.getId(state)));yield true;}
                    case "scheduleTick" -> null;
                    default -> throw new UnsupportedOperationException(method.toString());
                });
                var random=new LegacyRandomSource(17);building.postProcess(world,null,null,random,bounds,center,BlockPos.ZERO);
                var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("direction",direction.get2DDataValue());item.addProperty("mode",mode);item.add("chunk",StructureGolden.ints(cx,cz));item.add("writes",writes);item.addProperty("next",random.nextInt());out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("monument_building_blocks.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(Path.of(args[0]));}
}
