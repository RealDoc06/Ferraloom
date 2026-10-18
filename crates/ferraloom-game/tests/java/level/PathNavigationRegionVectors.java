import java.util.*;
import java.util.function.BooleanSupplier;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.piston.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.lighting.LevelLightEngine;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.CollisionContext;

/** Native PathNavigationRegion over real LevelChunks. Only the nullable chunk
 * source is controlled. Loading and live block reads are forbidden during a
 * captured query; all region and collision code is the real 26.3 implementation. */
public final class PathNavigationRegionVectors extends NeighborFixture {
    final Map<ChunkPos,LevelChunk> loaded=new HashMap<>();
    final List<String> now=new ArrayList<>();
    final WorldBorder border=new WorldBorder();
    final Source source=new Source();
    PathNavigationRegionVectors(RegistryAccess registry,String chunks){
        super(registry,1000);
        for(String key:chunks.split(";"))if(!key.isEmpty()){
            var xy=key.split(",");add(new ChunkPos(Integer.parseInt(xy[0]),Integer.parseInt(xy[1])));
        }
    }
    void add(ChunkPos pos){loaded.put(pos,new LevelChunk(this,pos));}
    final class Source extends ChunkSource {
        final LevelLightEngine light=new LevelLightEngine(this,false,false);
        @Override public BlockGetter getLevel(){return PathNavigationRegionVectors.this;}
        @Override public LevelChunk getChunkNow(int x,int z){now.add(x+","+z);return loaded.get(new ChunkPos(x,z));}
        @Override public ChunkAccess getChunk(int x,int z,ChunkStatus status,boolean load){throw new AssertionError("loading read escaped the captured region");}
        @Override public void tick(BooleanSupplier time,boolean chunks){}
        @Override public String gatherStats(){return "fixture";}
        @Override public int getLoadedChunksCount(){return loaded.size();}
        @Override public LevelLightEngine getLightEngine(){return light;}
    }
    @Override public ChunkSource getChunkSource(){return source;}
    @Override public WorldBorder getWorldBorder(){return border;}
    @Override public BlockState getBlockState(BlockPos pos){throw new AssertionError("live block read escaped the captured region");}
    void set(BlockPos pos,BlockState state){
        var chunk=loaded.get(new ChunkPos(pos.getX()>>4,pos.getZ()>>4));
        chunk.getSection(chunk.getSectionIndex(pos.getY())).setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,state);
    }
    static String p(BlockPos pos){return pos.getX()+","+pos.getY()+","+pos.getZ();}
    static String bb(AABB box){return box.minX+","+box.minY+","+box.minZ+","+box.maxX+","+box.maxY+","+box.maxZ;}
    static class Captured extends PathNavigationRegion {
        Captured(Level level,BlockPos start,BlockPos end){super(level,start,end);}
        boolean empty(){return allEmpty;}
    }
    Captured capture(BlockPos start,BlockPos end){now.clear();return new Captured(this,start,end);}
    void record(String name,String chunks,BlockPos start,BlockPos end,List<BlockPos> positions){
        var region=capture(start,end);
        var values=new ArrayList<String>();
        for(var pos:positions)values.add(Block.getId(region.getBlockState(pos))+","+BuiltInRegistries.FLUID.getId(region.getFluidState(pos).getType()));
        System.out.println("capture\t"+name+"\t"+chunks+"\t"+p(start)+"\t"+p(end)+"\t"+positions.stream().map(PathNavigationRegionVectors::p).collect(Collectors.joining(";"))+"\t"+String.join(";",values)+"\t"+region.empty()+"\t"+String.join(";",now));
    }
    static void mutations(RegistryAccess registry){
        for(String mutation:List.of("late-load","block-change","unload-replace")){
            var level=new PathNavigationRegionVectors(registry,mutation.equals("late-load")?"":"0,0");var pos=new BlockPos(8,64,8);
            if(!mutation.equals("late-load"))level.set(pos,Blocks.STONE.defaultBlockState());
            var region=level.capture(new BlockPos(0,64,0),new BlockPos(15,64,15));
            if(mutation.equals("block-change"))level.set(pos,Blocks.DIRT.defaultBlockState());
            else {level.loaded.remove(new ChunkPos(0,0));level.add(new ChunkPos(0,0));level.set(pos,Blocks.DIRT.defaultBlockState());}
            System.out.println("mutation\t"+mutation+"\t"+Block.getId(region.getBlockState(pos))+"\t"+String.join(";",level.now));
        }
    }
    static void collisions(RegistryAccess registry){
        for(String kind:List.of("stone","slab","powder","absent","border"))for(boolean withEntity:new boolean[]{false,true}){
            var level=new PathNavigationRegionVectors(registry,"0,0;1,0");
            var pos=kind.equals("absent")?new BlockPos(16,64,8):new BlockPos(8,64,8);
            var state=switch(kind){case "slab"->Blocks.STONE_SLAB.defaultBlockState();case "powder"->Blocks.POWDER_SNOW.defaultBlockState();case "border"->Blocks.AIR.defaultBlockState();default->Blocks.STONE.defaultBlockState();};
            level.set(pos,state);level.border.setSize(kind.equals("border")?10:59999968);
            var item=new ItemEntity(level,kind.equals("border")?4.5:8.5,64,kind.equals("border")?.5:8.5,new ItemStack(Items.STONE));item.fallDistance=3;
            var region=level.capture(new BlockPos(0,0,0),new BlockPos(15,319,15));
            Entity source=withEntity?item:null;
            var area=kind.equals("border")?new AABB(4.8,64,.1,5.2,64.5,.9):new AABB(pos.getX()+.1,64.6,8.1,pos.getX()+.9,64.8,8.9);
            boolean clear=region.noCollision(source,area);
            double top=region.getBlockState(pos).getCollisionShape(region,pos,CollisionContext.empty()).isEmpty()?0:region.getBlockState(pos).getCollisionShape(region,pos,CollisionContext.empty()).max(Direction.Axis.Y);
            System.out.println("collision\t"+kind+"\t"+withEntity+"\t"+bb(area)+"\t"+clear+"\t"+top+"\t"+region.getEntityCollisions(source,area).size()+"\t"+String.join(";",level.now));
        }
        var level=new PathNavigationRegionVectors(registry,"0,0;1,0");var pos=new BlockPos(16,64,8);var state=Blocks.MOVING_PISTON.defaultBlockState();
        level.set(pos,state);var chunk=level.loaded.get(new ChunkPos(1,0));
        var piston=new PistonMovingBlockEntity(pos,state,Blocks.STONE.defaultBlockState(),Direction.EAST,true,false);chunk.setBlockEntity(piston);
        for(int end:new int[]{15,31}){
            var region=level.capture(new BlockPos(0,64,0),new BlockPos(end,64,15));
            System.out.println("piston\t"+end+"\t"+state.getCollisionShape(region,pos,CollisionContext.empty()).isEmpty());
        }
    }
    public static void captureVectors(RegistryAccess registry){
        var level=new PathNavigationRegionVectors(registry,"0,0;1,0");level.set(new BlockPos(8,64,8),Blocks.STONE.defaultBlockState());level.set(new BlockPos(16,64,8),Blocks.WATER.defaultBlockState());
        level.record("bounded","0,0;1,0",new BlockPos(0,64,0),new BlockPos(15,64,15),List.of(new BlockPos(8,64,8),new BlockPos(16,64,8),new BlockPos(-1,64,8),new BlockPos(8,-65,8),new BlockPos(8,320,8)));
        var empty=new PathNavigationRegionVectors(registry,"");empty.record("absent","",new BlockPos(0,64,0),new BlockPos(15,64,15),List.of(new BlockPos(8,64,8),new BlockPos(8,-65,8)));
        var order=new PathNavigationRegionVectors(registry,"0,0");order.record("x-major","0,0",new BlockPos(-1,64,-1),new BlockPos(16,64,16),List.of(new BlockPos(0,64,0)));
        for(int start:new int[]{63,64}){
            var y=new PathNavigationRegionVectors(registry,"0,0");y.set(new BlockPos(8,64,8),Blocks.STONE.defaultBlockState());
            y.record("section-stepping-"+start,"0,0",new BlockPos(0,start,0),new BlockPos(15,64,15),List.of(new BlockPos(8,64,8)));
        }
        mutations(registry);collisions(registry);
    }
}

class PathNavigationRegionCapture {
    public static void main(String[] args) {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var context=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        PathNavigationRegionVectors.captureVectors(new RegistryAccess.ImmutableRegistryAccess(Stream.concat(builtins.registries(),registries.registries())).freeze());
    }
}
