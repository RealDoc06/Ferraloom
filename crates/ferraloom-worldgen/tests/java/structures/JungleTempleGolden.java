import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;

/** Saved temples with populated containers isolate geometry and moss RNG. */
public final class JungleTempleGolden {
    static int ground(int mode,int x,int z){return mode==0?63:mode==1?60+Math.floorMod(x*3+z*5,8):55;}
    static BlockState original(int mode,BlockPos p){if(p.getY()<=ground(mode,p.getX(),p.getZ()))return Blocks.STONE.defaultBlockState();return mode==2&&p.getY()<=63?Blocks.WATER.defaultBlockState():Blocks.AIR.defaultBlockState();}
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var containers=PalettedContainerFactory.create(registries);
        var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        for(var direction:Direction.Plane.HORIZONTAL)for(int mode=0;mode<3;mode++)for(boolean clip:new boolean[]{false,true}) {
            int terrain=mode;int sx=direction.getAxis()==Direction.Axis.Z?12:15;int sz=direction.getAxis()==Direction.Axis.Z?15:12;
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{id:'minecraft:tejp',GD:0,O:"+direction.get2DDataValue()+",BB:[I;0,64,0,"+(sx-1)+",73,"+(sz-1)+"],Width:12,Height:10,Depth:15,HPos:-1,placedMainChest:true,placedHiddenChest:true,placedTrap1:true,placedTrap2:true}");
            var piece=new net.minecraft.world.level.levelgen.structure.structures.JungleTemplePiece(tag);
            var changed=new HashMap<BlockPos,BlockState>();var writes=new JsonArray();var chunk=new ProtoChunk(new ChunkPos(0,0),UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),containers,null);
            var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(JungleTempleGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->{
                return switch(method.getName()) {
                    case "getHeightmapPos" -> {var p=(BlockPos)args[1];yield new BlockPos(p.getX(),terrain==2?64:ground(terrain,p.getX(),p.getZ())+1,p.getZ());}
                    case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0]));
                    case "getFluidState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0])).getFluidState();
                    case "getMinY" -> -64;
                    case "getChunk" -> chunk;
                    case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);if(((Integer)args[2]&16)==0){var post=state.getPostProcessPos((WorldGenLevel)proxy,p);if(post!=null)chunk.markPosForPostProcessing(post);}yield true;}
                    default -> throw new UnsupportedOperationException(method.toString());
                };
            });
            var bounds=new BoundingBox(0,-63,0,clip?3:15,319,15);var random=new LegacyRandomSource(17);piece.postProcess(world,null,null,random,bounds,new ChunkPos(0,0),BlockPos.ZERO);
            var item=new JsonObject();item.addProperty("direction",direction.get2DDataValue());item.addProperty("mode",mode);item.add("bounds",StructureGolden.bounds(bounds));item.addProperty("initial",tag.toString());item.addProperty("nbt",piece.createTag(serialization).toString());item.add("writes",writes);item.addProperty("next",random.nextInt());
            var post=new JsonArray();var sections=chunk.getPostProcessing();for(int i=0;i<sections.length;i++)if(sections[i]!=null)for(short packed:sections[i]){var p=ProtoChunk.unpackOffsetCoordinates(packed,i-4,new ChunkPos(0,0));post.add(StructureGolden.ints(p.getX(),p.getY(),p.getZ()));}item.add("post",post);out.add(item);
        }
        StructureGolden.write(directory.resolve("jungle_temple_blocks.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
