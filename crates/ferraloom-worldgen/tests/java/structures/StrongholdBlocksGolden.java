import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.ticks.*;

/** Saved stronghold pieces with strict null container access and controlled terrain. */
public final class StrongholdBlocksGolden {
    record Case(String id,int width,int height,int depth,String extra) {}
    static BlockState original(int mode,BlockPos p) {
        if(p.getY()>100)return Blocks.AIR.defaultBlockState();
        if(mode==1&&p.getY()>=50&&p.getY()<=65) {
            int v=p.getX()*3+p.getY()*5+p.getZ()*7;
            if(Math.floorMod(v,11)==0)return Blocks.AIR.defaultBlockState();
            if(Math.floorMod(v,17)==0)return Blocks.WATER.defaultBlockState();
        }
        return Blocks.STONE.defaultBlockState();
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var cases=new ArrayList<Case>();
        cases.add(new Case("shstart",5,11,5,"Source:true"));cases.add(new Case("shsd",5,11,5,"Source:false"));
        for(int mask=0;mask<4;mask++)cases.add(new Case("shs",5,5,7,"Left:"+((mask&1)!=0)+",Right:"+((mask&2)!=0)));
        cases.add(new Case("shph",9,5,11,""));cases.add(new Case("shlt",5,5,5,""));cases.add(new Case("shrt",5,5,5,""));
        for(int style=0;style<5;style++)cases.add(new Case("shrc",11,7,11,"Type:"+style));
        cases.add(new Case("shssd",5,11,8,""));
        for(int mask=0;mask<16;mask++)cases.add(new Case("sh5c",10,9,11,"leftLow:"+((mask&1)!=0)+",leftHigh:"+((mask&2)!=0)+",rightLow:"+((mask&4)!=0)+",rightHigh:"+((mask&8)!=0)));
        cases.add(new Case("shcc",5,5,7,"Chest:true"));cases.add(new Case("shli",14,6,15,"Tall:false"));cases.add(new Case("shli",14,11,15,"Tall:true"));cases.add(new Case("shpr",11,8,16,"Mob:true"));
        for(int steps=1;steps<=3;steps++)cases.add(new Case("shfc",5,5,steps,"Steps:"+steps));
        var containers=PalettedContainerFactory.create(registries);var context=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null);
        var gson=new Gson();boolean first=true;
        try(var writer=new java.io.OutputStreamWriter(new java.util.zip.GZIPOutputStream(Files.newOutputStream(directory.resolve("stronghold_blocks.json.gz"))),java.nio.charset.StandardCharsets.UTF_8)) {
            writer.write('[');
            for(var scenario:cases)for(var direction:Direction.Plane.HORIZONTAL)for(String door:List.of("OPENING","WOOD_DOOR","GRATES","IRON_DOOR"))for(boolean clip:new boolean[]{false,true}) {
                int mode=(direction.get2DDataValue()+(door.equals("OPENING")||door.equals("GRATES")?0:1))%2;
                boolean axisZ=direction.getAxis()==Direction.Axis.Z;int sx=axisZ?scenario.width():scenario.depth(),sz=axisZ?scenario.depth():scenario.width();
                var tag=TagParser.parseCompoundFully("{id:'minecraft:"+scenario.id()+"',BB:[I;0,50,0,"+(sx-1)+","+(49+scenario.height())+","+(sz-1)+"],GD:2,O:"+direction.get2DDataValue()+",EntryDoor:'"+door+"'"+(scenario.extra().isEmpty()?"":","+scenario.extra())+"}");
                var list=new ListTag();list.add(tag);var piece=PiecesContainer.load(list,context).pieces().getFirst();
                var item=place(registries,containers,context,piece,tag,mode,clip,new LegacyRandomSource(17));
                if(!first)writer.write(',');first=false;gson.toJson(item,writer);
            }
            writer.write(']');
        }
        var frames=new JsonArray();
        for(var direction:Direction.Plane.HORIZONTAL)for(boolean clip:new boolean[]{false,true}) {
            boolean axisZ=direction.getAxis()==Direction.Axis.Z;
            var tag=TagParser.parseCompoundFully("{id:'minecraft:shpr',BB:[I;0,50,0,"+(axisZ?10:15)+",57,"+(axisZ?15:10)+"],GD:2,O:"+direction.get2DDataValue()+",EntryDoor:'OPENING',Mob:true}");
            var list=new ListTag();list.add(tag);var piece=PiecesContainer.load(list,context).pieces().getFirst();
            var random=new LegacyRandomSource(17) {@Override public float nextFloat(){return Math.nextDown(1.0F);}};
            var item=place(registries,containers,context,piece,tag,0,clip,random);
            var onlyFrames=new JsonArray();for(var write:item.getAsJsonArray("writes")){var state=Block.BLOCK_STATE_REGISTRY.byId(write.getAsJsonObject().get("state").getAsInt());if(state.is(Blocks.END_PORTAL_FRAME)||state.is(Blocks.END_PORTAL))onlyFrames.add(write);}item.add("writes",onlyFrames);frames.add(item);
        }
        StructureGolden.write(directory.resolve("stronghold_portal_frames.json.gz"),frames);
    }
    static JsonObject place(RegistryAccess.Frozen registries,PalettedContainerFactory containers,StructurePieceSerializationContext context,StructurePiece piece,CompoundTag tag,int mode,boolean clip,net.minecraft.util.RandomSource random)throws Exception {
        var bounds=new BoundingBox(0,-63,0,clip?7:15,319,15);var changed=new HashMap<BlockPos,BlockState>();var writes=new JsonArray();var chunk=new ProtoChunk(new ChunkPos(0,0),UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),containers,null);var ticks=new ProtoChunkTicks<net.minecraft.world.level.material.Fluid>();
        var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(StrongholdBlocksGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
            case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],original(mode,(BlockPos)args[0]));
            case "getFluidState" -> changed.getOrDefault((BlockPos)args[0],original(mode,(BlockPos)args[0])).getFluidState();
            case "getChunk" -> chunk;
            case "getBlockEntity" -> null;
            case "scheduleTick" -> {ticks.schedule(new ScheduledTick<>((net.minecraft.world.level.material.Fluid)args[1],(BlockPos)args[0],0L,0L));yield null;}
            case "setBlock" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);if(((Integer)args[2]&16)==0){var post=state.getPostProcessPos((WorldGenLevel)proxy,p);if(post!=null)chunk.markPosForPostProcessing(post);}yield true;}
            default -> throw new UnsupportedOperationException(method.toString());
        });
        piece.postProcess(world,null,null,random,bounds,new ChunkPos(0,0),BlockPos.ZERO);
        var item=new JsonObject();item.addProperty("id",tag.getStringOr("id",""));item.addProperty("mode",mode);item.add("bounds",StructureGolden.bounds(bounds));item.addProperty("initial",tag.toString());item.addProperty("nbt",piece.createTag(context).toString());item.add("writes",writes);item.addProperty("next",random.nextInt());
        var post=new JsonArray();var sections=chunk.getPostProcessing();for(int i=0;i<sections.length;i++)if(sections[i]!=null)for(short packed:sections[i]){var p=ProtoChunk.unpackOffsetCoordinates(packed,i-4,new ChunkPos(0,0));post.add(StructureGolden.ints(p.getX(),p.getY(),p.getZ()));}item.add("post",post);
        var fluid=new JsonArray();for(var tick:ticks.pack(0)){fluid.add(StructureGolden.ints(tick.pos().getX(),tick.pos().getY(),tick.pos().getZ(),BuiltInRegistries.FLUID.getId(tick.type()),tick.priority().getValue()));}item.add("fluid_ticks",fluid);return item;
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
