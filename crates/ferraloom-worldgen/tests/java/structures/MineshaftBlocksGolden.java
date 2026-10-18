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
import net.minecraft.world.level.levelgen.structure.structures.*;

/** Saved pieces cover orientation, clipping and preserved supports. Entity paths remain pending. */
public final class MineshaftBlocksGolden {
    static BlockState original(int mode,BlockPos p) {
        if(mode==1&&p.equals(new BlockPos(1,49,1)))return Blocks.WATER.defaultBlockState();
        if(p.getY()>100)return Blocks.AIR.defaultBlockState();
        if(mode==2&&p.getY()>=49&&p.getY()<=58&&Math.floorMod(p.getX()*3+p.getZ()*7,13)==0)return switch(Math.floorMod(p.getX()+p.getZ(),4)) {case 0->Blocks.OAK_PLANKS.defaultBlockState();case 1->Blocks.OAK_LOG.defaultBlockState();case 2->Blocks.OAK_FENCE.defaultBlockState();default->Blocks.IRON_CHAIN.defaultBlockState();};
        return Blocks.STONE.defaultBlockState();
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var containers=PalettedContainerFactory.create(registries);var biome=registries.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS);
        var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        for(var direction:Direction.Plane.HORIZONTAL)for(int kind=0;kind<7;kind++)for(int mode=0;mode<3;mode++)for(boolean clip:new boolean[]{false,true}) {
            boolean axisZ=direction.getAxis()==Direction.Axis.Z;
            String bb,suffix,id;int orientation=direction.get2DDataValue();
            switch(kind) {
                case 0 -> {id="msroom";bb="0,50,0,11,58,14";orientation=-1;suffix="Entrances:[[I;2,51,0,4,53,1],[I;10,52,6,11,54,8]]";}
                case 1,2 -> {id="mscrossing";bb="2,50,2,6,"+(kind==1?52:56)+",6";orientation=-1;suffix="D:"+direction.get2DDataValue()+",tf:"+(kind==2);}
                case 3 -> {id="msstairs";bb="0,45,0,"+(axisZ?2:8)+",52,"+(axisZ?8:2);suffix="";}
                default -> {id="mscorridor";bb="0,50,0,"+(axisZ?2:9)+",52,"+(axisZ?9:2);suffix="hr:"+(kind==5)+",sc:"+(kind==6)+",hps:true,Num:2";}
            }
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{id:'minecraft:"+id+"',BB:[I;"+bb+"],GD:2,O:"+orientation+",MST:0"+(suffix.isEmpty()?"":","+suffix)+"}");
            StructurePiece piece=switch(kind) {case 0->new MineshaftPieces.MineShaftRoom(tag);case 1,2->new MineshaftPieces.MineShaftCrossing(tag);case 3->new MineshaftPieces.MineShaftStairs(tag);default->new MineshaftPieces.MineShaftCorridor(tag);};
            var changed=new HashMap<BlockPos,BlockState>();var writes=new JsonArray();var chunk=new ProtoChunk(new ChunkPos(0,0),UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),containers,null);int terrain=mode;
            var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(MineshaftBlocksGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
                case "getBiome" -> biome;
                case "getHeight" -> 101;
                case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0]));
                case "getFluidState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0])).getFluidState();
                case "getMinY" -> -64;
                case "getMaxY" -> 319;
                case "getChunk" -> chunk;
                case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);if(((Integer)args[2]&16)==0){var post=state.getPostProcessPos((WorldGenLevel)proxy,p);if(post!=null)chunk.markPosForPostProcessing(post);}yield true;}
                default -> throw new UnsupportedOperationException(method.toString());
            });
            var bounds=new BoundingBox(0,-63,0,clip?6:15,319,15);var random=new LegacyRandomSource(17);boolean pending=false;
            try {piece.postProcess(world,null,null,random,bounds,new ChunkPos(0,0),BlockPos.ZERO);}catch(UnsupportedOperationException e) {if(!e.getMessage().contains("getLevel"))throw e;pending=true;}
            var item=new JsonObject();item.addProperty("id","minecraft:"+id);item.addProperty("kind",kind);item.addProperty("mode",mode);item.addProperty("pending",pending);item.add("bounds",StructureGolden.bounds(bounds));item.addProperty("initial",tag.toString());item.addProperty("nbt",piece.createTag(serialization).toString());item.add("writes",writes);item.addProperty("next",random.nextInt());
            var post=new JsonArray();var sections=chunk.getPostProcessing();for(int i=0;i<sections.length;i++)if(sections[i]!=null)for(short packed:sections[i]){var p=ProtoChunk.unpackOffsetCoordinates(packed,i-4,new ChunkPos(0,0));post.add(StructureGolden.ints(p.getX(),p.getY(),p.getZ()));}item.add("post",post);out.add(item);
        }
        StructureGolden.write(directory.resolve("mineshaft_blocks.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
