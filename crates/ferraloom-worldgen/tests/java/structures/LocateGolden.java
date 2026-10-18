import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import org.mockito.Mockito;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.placement.*;

/** Run the real public ChunkGenerator locate algorithm; mock only synchronous IO. */
public final class LocateGolden {
    static JsonArray event(Object...values) {var out=new JsonArray();for(Object v:values) {if(v instanceof String s)out.add(s);else if(v instanceof Boolean b)out.add(b);else out.add((Number)v);}return out;}
    static int hash(int x,int z,int id,int mode){return Math.floorMod(x*31+z*17+id*13+mode*7,11);}
    static int presence(int x,int z,int id,int mode){int h=hash(x,z,id,mode);return mode==0?1:mode==1?(h<8?0:2):h<4?1:h<7?0:2;}
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var registry=registries.lookupOrThrow(Registries.STRUCTURE);String[] names={"stronghold","village_plains","mineshaft","pillager_outpost","igloo"};
        var holders=new ArrayList<Holder<Structure>>();for(String name:names)holders.add(registry.getOrThrow(ResourceKey.create(Registries.STRUCTURE,Identifier.withDefaultNamespace(name))));
        var indices=new IdentityHashMap<Structure,Integer>();for(int i=0;i<holders.size();i++)indices.put(holders.get(i).value(),i);
        String[] definitions={"{type:'minecraft:random_spread',spacing:11,separation:3,salt:345,locate_offset:[3,7,-5]}","{type:'minecraft:random_spread',spacing:17,separation:7,spread_type:'triangular',salt:777,locate_offset:[-6,11,9]}","{type:'minecraft:concentric_rings',salt:0,distance:4,spread:2,count:6,preferred_biomes:[],locate_offset:[7,-13,-2]}","{type:'minecraft:dimension_origin'}"};
        var ops=RegistryOps.create(JsonOps.INSTANCE,registries);var placements=new ArrayList<StructurePlacement>();for(String text:definitions)placements.add(StructurePlacement.CODEC.parse(ops,JsonParser.parseString(text)).getOrThrow());
        var byStructure=List.of(List.of(2),List.of(0,2),List.of(0,1),List.of(3),List.<Integer>of());
        var rings=List.of(new ChunkPos(7,-4),new ChunkPos(-3,6),new ChunkPos(7,4),new ChunkPos(-3,-6),new ChunkPos(0,0),new ChunkPos(20,-20));
        var biomes=registries.lookupOrThrow(Registries.BIOME);var generator=new FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var out=new JsonObject();var defs=new JsonArray();for(String definition:definitions)defs.add(JsonParser.parseString(definition));out.add("placements",defs);var ringJson=new JsonArray();for(var p:rings)ringJson.add(StructureGolden.ints(p.x(),p.z()));out.add("rings",ringJson);var cases=new JsonArray();
        for(long seed:new long[]{0,-1})for(int[] position:new int[][]{{0,64,0},{-17,-64,31},{135,32,-271},{120,32,8},{Integer.MAX_VALUE,70,Integer.MIN_VALUE}})for(int scenario=0;scenario<4;scenario++)for(int radius:new int[]{-1,0,2})for(boolean reference:new boolean[]{false,true})for(boolean enabled:new boolean[]{false,true})for(int[] wanted:new int[][]{{0,1,2,3},{2,1,0},{0},{3,4}}) {
            int mode=scenario;var trace=new JsonArray();org.mockito.stubbing.Answer<Object> forbidden=call->{throw new UnsupportedOperationException("Unapproved locate boundary call: "+call.getMethod());};
            var level=Mockito.mock(ServerLevel.class,forbidden);var server=Mockito.mock(MinecraftServer.class,forbidden);var settings=Mockito.mock(WorldGenSettings.class,forbidden);var source=Mockito.mock(ServerChunkCache.class,forbidden);var state=Mockito.mock(ChunkGeneratorStructureState.class,forbidden);var manager=Mockito.mock(StructureManager.class,forbidden);var chunk=Mockito.mock(ChunkAccess.class,forbidden);
            Mockito.doReturn(server).when(level).getServer();Mockito.doReturn(settings).when(server).getWorldGenSettings();Mockito.doReturn(new WorldOptions(seed,enabled,false)).when(settings).options();Mockito.doReturn(source).when(level).getChunkSource();Mockito.doReturn(state).when(source).getGeneratorState();Mockito.doReturn(manager).when(level).structureManager();Mockito.doReturn(seed).when(state).getLevelSeed();
            Mockito.doAnswer(call->{var structure=(Holder<Structure>)call.getArgument(0);var list=new ArrayList<StructurePlacement>();for(int i:byStructure.get(indices.get(structure.value())))list.add(placements.get(i));return list;}).when(state).getPlacementsForStructure(Mockito.any());
            Mockito.doAnswer(call->mode==3?null:rings).when(state).getRingPositionsFor(Mockito.any());
            var target=new ChunkPos[1];var loaded=new HashMap<String,StructureStart>();
            Mockito.doAnswer(call->{var p=(ChunkPos)call.getArgument(0);int id=indices.get((Structure)call.getArgument(1));var placement=(StructurePlacement)call.getArgument(2);boolean require=call.getArgument(3);int result=presence(p.x(),p.z(),id,mode);trace.add(event("check",id,p.x(),p.z(),placements.indexOf(placement),require,result));return StructureCheckResult.values()[result];}).when(manager).checkStructurePresence(Mockito.any(),Mockito.any(),Mockito.any(),Mockito.anyBoolean());
            Mockito.doAnswer(call->{int x=call.getArgument(0),z=call.getArgument(1);if(call.getArgument(2)!=ChunkStatus.STRUCTURE_STARTS)throw new AssertionError("chunk status");target[0]=new ChunkPos(x,z);trace.add(event("chunk",x,z));return chunk;}).when(level).getChunk(Mockito.anyInt(),Mockito.anyInt(),Mockito.any(ChunkStatus.class));
            Mockito.doAnswer(call->{int id=indices.get((Structure)call.getArgument(0));var p=target[0];trace.add(event("load",id,p.x(),p.z()));String key=id+":"+p.x()+":"+p.z();if(loaded.containsKey(key))return loaded.get(key);int h=Math.floorMod(p.x()*7+p.z()*3+id+mode,7);if(h==0)return null;if(h==1)return StructureStart.INVALID_START;int refs=h==2?1:h==3?2:h==4?-1:0;var origin=new ChunkPos(p.x()+(h==5?1:0),p.z()-(h==6?1:0));var result=new StructureStart(holders.get(id).value(),origin,refs,new PiecesContainer(List.of(new net.minecraft.world.level.levelgen.structure.structures.BuriedTreasurePieces.BuriedTreasurePiece(BlockPos.ZERO))));loaded.put(key,result);return result;}).when(manager).getStartForStructure(Mockito.any(),Mockito.any());
            Mockito.doAnswer(call->{var start=(StructureStart)call.getArgument(0);start.addReference();trace.add(event("reference",indices.get(start.getStructure()),start.getChunkPos().x(),start.getChunkPos().z(),start.getReferences()));return null;}).when(manager).addReference(Mockito.any());
            var item=new JsonObject();item.addProperty("seed",seed);item.add("position",StructureGolden.ints(position));item.addProperty("mode",mode);item.addProperty("radius",radius);item.addProperty("reference",reference);item.addProperty("enabled",enabled);item.add("wanted",StructureGolden.ints(wanted));
            try {var result=generator.findNearestMapStructure(level,HolderSet.direct(Arrays.stream(wanted).mapToObj(holders::get).toList()),new BlockPos(position[0],position[1],position[2]),radius,reference);if(result==null)item.addProperty("outcome","none");else {item.addProperty("outcome","found");var p=result.getFirst();item.add("result",StructureGolden.ints(p.getX(),p.getY(),p.getZ(),indices.get(result.getSecond().value())));}}
            catch(IllegalStateException error) {if(mode!=3)throw error;item.addProperty("outcome","throws");}
            item.add("trace",trace);cases.add(item);Mockito.framework().clearInlineMocks();
        }
        out.add("cases",cases);StructureGolden.write(directory.resolve("locate.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
