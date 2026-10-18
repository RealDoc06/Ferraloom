import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;

/** Exact geometry and archaeology ordering with separate controlled RNGs. */
public final class DesertPyramidGolden {
    static int ground(int mode,int x,int z){return mode==0?63:60+Math.floorMod(x*3+z*5,8);}
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var containers=PalettedContainerFactory.create(registries);
        var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        var structure=registries.lookupOrThrow(Registries.STRUCTURE).getValue(Identifier.withDefaultNamespace("desert_pyramid"));
        for(var direction:Direction.Plane.HORIZONTAL)for(int mode=0;mode<2;mode++)for(int quadrant=0;quadrant<2;quadrant++)for(long seed:new long[]{0,-1,8675309})for(boolean mixed:new boolean[]{false,true}) {
            int terrain=mode;var center=new ChunkPos(quadrant,quadrant);var bounds=new BoundingBox(center.getMinBlockX(),-63,center.getMinBlockZ(),center.getMaxBlockX(),319,center.getMaxBlockZ());
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{id:'minecraft:tedp',GD:0,O:"+direction.get2DDataValue()+",BB:[I;0,64,0,20,78,20],Width:21,Height:15,Depth:21,HPos:-1,hasPlacedChest0:true,hasPlacedChest1:true,hasPlacedChest2:true,hasPlacedChest3:true}");
            var piece=new net.minecraft.world.level.levelgen.structure.structures.DesertPyramidPiece(tag);
            var changed=new HashMap<BlockPos,BlockState>();var writes=new JsonArray();var chunk=new ProtoChunk(center,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),containers,null);
            var regionRandom=new LegacyRandomSource(777);
            var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(DesertPyramidGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->{
                return switch(method.getName()) {
                    case "getHeightmapPos" -> {var p=(BlockPos)args[1];yield new BlockPos(p.getX(),ground(terrain,p.getX(),p.getZ())+1,p.getZ());}
                    case "getBlockState" -> {var p=(BlockPos)args[0];yield changed.getOrDefault(p,p.getY()<=ground(terrain,p.getX(),p.getZ())?Blocks.STONE.defaultBlockState():Blocks.AIR.defaultBlockState());}
                    case "getFluidState" -> {var p=(BlockPos)args[0];yield changed.getOrDefault(p,p.getY()<=ground(terrain,p.getX(),p.getZ())?Blocks.STONE.defaultBlockState():Blocks.AIR.defaultBlockState()).getFluidState();}
                    case "getMinY" -> -64;
                    case "getMaxY" -> 319;
                    case "getSeed" -> seed;
                    case "getRandom" -> regionRandom;
                    case "getBlockEntity" -> args.length==2?Optional.empty():null;
                    case "getChunk" -> chunk;
                    case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);if(((Integer)args[2]&16)==0){var post=state.getPostProcessPos((WorldGenLevel)proxy,p);if(post!=null)chunk.markPosForPostProcessing(post);}yield true;}
                    default -> throw new UnsupportedOperationException(method.toString());
                };
            });
            var random=new LegacyRandomSource(17);piece.postProcess(world,null,null,random,bounds,center,BlockPos.ZERO);
            var item=new JsonObject();item.addProperty("direction",direction.get2DDataValue());item.addProperty("mode",mode);item.addProperty("seed",seed);item.addProperty("mixed",mixed);item.add("center",StructureGolden.ints(quadrant,quadrant));item.add("bounds",StructureGolden.bounds(bounds));item.addProperty("initial",tag.toString());item.addProperty("nbt",piece.createTag(serialization).toString());item.add("writes",writes.deepCopy());item.addProperty("next",random.nextInt());item.addProperty("region_next",regionRandom.nextInt());
            var potential=new JsonArray();for(var p:piece.getPotentialSuspiciousSandWorldPositions())potential.add(StructureGolden.ints(p.getX(),p.getY(),p.getZ()));item.add("potential",potential);var roof=piece.getRandomCollapsedRoofPos();item.add("roof",StructureGolden.ints(roof.getX(),roof.getY(),roof.getZ()));
            writes.asList().clear();var pieces=new ArrayList<StructurePiece>();pieces.add(piece);if(mixed)pieces.add(new net.minecraft.world.level.levelgen.structure.structures.BuriedTreasurePieces.BuriedTreasurePiece(new BlockPos(100,20,-40)));
            structure.afterPlace(world,null,null,random,bounds,center,new PiecesContainer(pieces));item.add("archaeology",writes.deepCopy());out.add(item);
        }
        StructureGolden.write(directory.resolve("desert_pyramid_blocks.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
