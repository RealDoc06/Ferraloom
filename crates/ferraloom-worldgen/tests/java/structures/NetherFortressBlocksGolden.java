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
import net.minecraft.world.ticks.*;

/** Saved fortress pieces isolate all geometry, postprocessing and fluid ticks. */
public final class NetherFortressBlocksGolden {
    static int ground(int mode,int x,int z) {return mode==0?60:58+Math.floorMod(x*3+z*5,7);}
    static BlockState original(int mode,BlockPos p) {return p.getY()<=ground(mode,p.getX(),p.getZ())?Blocks.STONE.defaultBlockState():Blocks.AIR.defaultBlockState();}
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var containers=PalettedContainerFactory.create(registries);
        var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        String[] ids={"nestart","nebcr","nebs","nebef","nerc","nesr","nemt","nece","nesc","nescsc","nescrt","nesclt","neccs","nectb","necsr"};
        int[][] sizes={{19,10,19},{19,10,19},{5,10,19},{5,10,8},{7,9,7},{7,11,7},{7,8,9},{13,14,13},{5,7,5},{5,7,5},{5,7,5},{5,7,5},{5,14,10},{9,7,9},{13,14,13}};
        for(int kind=0;kind<ids.length;kind++)for(var direction:Direction.Plane.HORIZONTAL)for(int mode=0;mode<2;mode++)for(int quadrant=0;quadrant<2;quadrant++)for(boolean clip:new boolean[]{false,true}) {
            boolean axisZ=direction.getAxis()==Direction.Axis.Z;int[] size=sizes[kind];int sx=axisZ?size[0]:size[2],sz=axisZ?size[2]:size[0];
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{id:'minecraft:"+ids[kind]+"',BB:[I;0,64,0,"+(sx-1)+","+(63+size[1])+","+(sz-1)+"],GD:2,O:"+direction.get2DDataValue()+",Chest:false,Mob:true,Seed:-8675309}");
            var list=new net.minecraft.nbt.ListTag();list.add(tag);var piece=PiecesContainer.load(list,serialization).pieces().getFirst();
            var center=new ChunkPos(quadrant,quadrant);int terrain=mode;
            var bounds=new BoundingBox(center.getMinBlockX(),-63,center.getMinBlockZ(),center.getMinBlockX()+(clip?7:15),319,center.getMaxBlockZ());
            var changed=new HashMap<BlockPos,BlockState>();var writes=new JsonArray();var chunk=new ProtoChunk(center,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),containers,null);var ticks=new ProtoChunkTicks<net.minecraft.world.level.material.Fluid>();
            var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(NetherFortressBlocksGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
                case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0]));
                case "getFluidState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0])).getFluidState();
                case "getMinY" -> -64;
                case "getChunk" -> chunk;
                case "scheduleTick" -> {ticks.schedule(new ScheduledTick<>((net.minecraft.world.level.material.Fluid)args[1],(BlockPos)args[0],0L,0L));yield null;}
                case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);if(((Integer)args[2]&16)==0){var post=state.getPostProcessPos((WorldGenLevel)proxy,p);if(post!=null)chunk.markPosForPostProcessing(post);}yield true;}
                default -> throw new UnsupportedOperationException(method.toString());
            });
            var random=new LegacyRandomSource(17);piece.postProcess(world,null,null,random,bounds,center,BlockPos.ZERO);
            var item=new JsonObject();item.addProperty("id","minecraft:"+ids[kind]);item.addProperty("mode",mode);item.add("center",StructureGolden.ints(center.x(),center.z()));item.add("bounds",StructureGolden.bounds(bounds));item.addProperty("initial",tag.toString());item.addProperty("nbt",piece.createTag(serialization).toString());item.add("writes",writes);item.addProperty("next",random.nextInt());
            var post=new JsonArray();var sections=chunk.getPostProcessing();for(int i=0;i<sections.length;i++)if(sections[i]!=null)for(short packed:sections[i]){var p=ProtoChunk.unpackOffsetCoordinates(packed,i-4,center);post.add(StructureGolden.ints(p.getX(),p.getY(),p.getZ()));}item.add("post",post);
            var fluid=new JsonArray();for(var tick:ticks.pack(0)){var a=StructureGolden.ints(tick.pos().getX(),tick.pos().getY(),tick.pos().getZ(),BuiltInRegistries.FLUID.getId(tick.type()),tick.priority().getValue());fluid.add(a);}item.add("fluid_ticks",fluid);out.add(item);
        }
        StructureGolden.write(directory.resolve("nether_fortress_blocks.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
