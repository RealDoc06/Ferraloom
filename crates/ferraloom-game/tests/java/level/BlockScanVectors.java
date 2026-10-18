import java.lang.reflect.*;
import java.util.*;
import java.util.function.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.blockscan.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.phys.AABB;

/** Real 26.3 matchers on real ProtoChunks. Only LevelReader's required chunk
 * service is bounded; its absent-chunk exception records the first required read.
 * The LevelReader defaults, palette pruning and scan/cache code are unmodified. */
public final class BlockScanVectors implements InvocationHandler {
    final Map<ChunkPos,ProtoChunk> chunks = new HashMap<>();
    final List<String> reads = new ArrayList<>();
    final LevelReader level;
    final PalettedContainerFactory factory;
    static String p(BlockPos p) { return p.getX()+","+p.getY()+","+p.getZ(); }
    static String positions(List<BlockPos> ps) { return ps.stream().map(BlockScanVectors::p).collect(Collectors.joining(";")); }
    BlockScanVectors(RegistryAccess registries, String loaded) {
        factory=PalettedContainerFactory.create(registries);
        level=(LevelReader)Proxy.newProxyInstance(LevelReader.class.getClassLoader(),new Class<?>[]{LevelReader.class},this);
        for(String key:loaded.split(";")) if(!key.isEmpty()) {
            String[] xy=key.split(","); var cp=new ChunkPos(Integer.parseInt(xy[0]),Integer.parseInt(xy[1]));
            chunks.put(cp,new ProtoChunk(cp,UpgradeData.EMPTY,LevelHeightAccessor.create(0,32),factory,null));
        }
    }
    @Override public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        if(method.getName().equals("getMinY"))return 0;
        if(method.getName().equals("getHeight")&&method.getParameterCount()==0)return 32;
        if(method.getName().equals("hasChunk"))return chunks.containsKey(new ChunkPos((int)args[0],(int)args[1]));
        if(method.getName().equals("getChunk")&&method.getParameterCount()==4) {
            if(args[2]!=ChunkStatus.FULL || !(boolean)args[3])throw new AssertionError("incorrect acquire contract");
            var cp=new ChunkPos((int)args[0],(int)args[1]); reads.add(cp.x()+","+cp.z());
            var chunk=chunks.get(cp); if(chunk==null)throw new IllegalStateException("missing:"+cp.x()+","+cp.z());
            return chunk;
        }
        if(method.getName().equals("getBlockState")) {
            var pos=(BlockPos)args[0]; if(pos.getY()<0||pos.getY()>31)return Blocks.AIR.defaultBlockState();
            return level.getChunk(pos).getBlockState(pos);
        }
        if(method.isDefault())return InvocationHandler.invokeDefault(proxy,method,args);
        throw new UnsupportedOperationException(method.toString());
    }
    void set(BlockPos p,BlockState state) {
        var chunk=chunks.get(new ChunkPos(p.getX()>>4,p.getZ()>>4));
        chunk.getSection(chunk.getSectionIndex(p.getY())).setBlockState(p.getX()&15,p.getY()&15,p.getZ()&15,state);
    }
    static Predicate<BlockState> filter(String name) {
        return switch(name) {
            case "fluid" -> s->!s.getFluidState().isEmpty();
            case "air" -> BlockState::isAir;
            case "solid" -> s->s.is(Blocks.STONE);
            case "none" -> s->false;
            default -> s->true;
        };
    }
    String states() {
        var entries=new ArrayList<String>();
        chunks.entrySet().stream().sorted(Comparator.<Map.Entry<ChunkPos,ProtoChunk>>comparingInt(e->e.getKey().x()).thenComparingInt(e->e.getKey().z())).forEach(e->{
            for(int y=0;y<32;y++)for(int z=0;z<16;z++)for(int x=0;x<16;x++){
                var p=new BlockPos(e.getKey().getMinBlockX()+x,y,e.getKey().getMinBlockZ()+z);
                var state=e.getValue().getBlockState(p); if(!state.isAir())entries.add(p(p)+","+Block.getId(state));
            }
        });
        return String.join(";",entries);
    }
    void box(String name,String loaded,String bounds,String filter,boolean liquid) {
        var b=Arrays.stream(bounds.split(",")).mapToDouble(Double::parseDouble).toArray();
        var box=new AABB(b[0],b[1],b[2],b[3],b[4],b[5]);
        String result;
        try {
            if(liquid)result=""+level.containsAnyLiquid(box);
            else {
                var found=new ArrayList<BlockPos>();
                level.findBlocksIn(box).filterState(filter(filter)).forEach((pos,state)->found.add(pos.immutable()));
                result=positions(found);
            }
        } catch(IllegalStateException e) { result=e.getMessage(); }
        System.out.println((liquid?"liquid":"box")+"\t"+name+"\t"+loaded+"\t"+states()+"\t"+bounds+"\t"+filter+"\t"+result+"\t"+String.join(";",reads));
    }
    void ordered(String name,String loaded,List<BlockPos> ps,String filter,boolean filterPos) {
        String result;
        try {
            var found=new ArrayList<BlockPos>();
            var matcher=level.findBlocksIn(ps).filterState(filter(filter));
            if(filterPos)matcher.filterPos(p->p.getX()<16);
            matcher.forEach((pos,state)->found.add(pos.immutable())); result=positions(found);
        } catch(IllegalStateException e){result=e.getMessage();}
        System.out.println("ordered\t"+name+"\t"+loaded+"\t"+states()+"\t"+positions(ps)+"\t"+filter+(filterPos?"-pos":"")+"\t"+result+"\t"+String.join(";",reads));
    }
    public static void main(String[] args) {
        var output=System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
        var registries=NeighborVectors.load();
        for(String state:List.of("air","stone","water","flowing-water","lava","wet-slab","dry-slab")) {
            var f=new BlockScanVectors(registries,"0,0");
            BlockState s=switch(state){case "stone"->Blocks.STONE.defaultBlockState();case "water"->Blocks.WATER.defaultBlockState();case "flowing-water"->Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,7);case "lava"->Blocks.LAVA.defaultBlockState();case "wet-slab"->Blocks.STONE_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true);case "dry-slab"->Blocks.STONE_SLAB.defaultBlockState();default->Blocks.AIR.defaultBlockState();};
            f.set(BlockPos.ZERO,s);f.box(state,"0,0","0,0,0,0,0,0","fluid",true);
        }
        for(boolean fluidFirst:List.of(false,true)) {
            var f=new BlockScanVectors(registries,"0,0");if(fluidFirst)f.set(new BlockPos(15,15,0),Blocks.WATER.defaultBlockState());
            f.box("unloaded-neighbor-"+fluidFirst,"0,0","15,15,0,16,16,0","fluid",true);
        }
        for(String bounds:List.of("0,-2,0,0,-1,0","0,32,0,0,33,0","16,-2,0,16,-1,0","16,32,0,16,33,0")) {
            var f=new BlockScanVectors(registries,"0,0");f.box("outside-height","0,0",bounds,"fluid",true);
        }
        var border=new BlockScanVectors(registries,"0,0;1,0");border.set(new BlockPos(16,16,0),Blocks.WATER.defaultBlockState());border.box("inclusive-upper-corner","0,0;1,0","15.25,15.25,0,16,16,0","fluid",true);
        var neg=new BlockScanVectors(registries,"-1,0;0,0");neg.set(new BlockPos(-1,0,0),Blocks.WATER.defaultBlockState());neg.box("negative-floor","-1,0;0,0","-0.01,0,0,0,0,0","fluid",true);
        var order=new BlockScanVectors(registries,"0,0;0,1;1,0;1,1");order.box("chunk-section-cell-order","0,0;0,1;1,0;1,1","15,15,15,16,16,16","all",false);
        var tail=new BlockScanVectors(registries,"0,0");tail.box("air-tail","0,0","0,-1,0,1,0,1","all",false);
        var far=new BlockScanVectors(registries,"0,0");far.box("air-wholly-below","0,0","0,-3,0,0,-2,0","air",false);
        var reversed=new BlockScanVectors(registries,"0,0");
        var r=new ArrayList<BlockPos>();reversed.level.findBlocksIn(new BlockPos(1,1,1),BlockPos.ZERO).forEach((p,s)->r.add(p.immutable()));
        System.out.println("reversed\t"+positions(r));
        var ps=List.of(new BlockPos(0,0,0),new BlockPos(1,0,0),new BlockPos(16,0,0),new BlockPos(0,16,0),new BlockPos(32,0,0),new BlockPos(0,0,0));
        for(String predicate:List.of("all","none","fluid")){
            var f=new BlockScanVectors(registries,"0,0;1,0;2,0"); f.set(BlockPos.ZERO,Blocks.WATER.defaultBlockState());f.ordered("cache-collision-"+predicate,"0,0;1,0;2,0",ps,predicate,false);
        }
        var missing=new BlockScanVectors(registries,"0,0");missing.ordered("missing","0,0",ps,"all",false);
        var filtered=new BlockScanVectors(registries,"0,0");filtered.ordered("filter-before-read","0,0",List.of(new BlockPos(16,0,0),BlockPos.ZERO),"all",true);
        var outside=new BlockScanVectors(registries,"");outside.ordered("outside-no-read","",List.of(new BlockPos(123,-1,456),new BlockPos(123,32,456)),"air",false);
        var queries=new BlockScanVectors(registries,"0,0");queries.set(BlockPos.ZERO,Blocks.WATER.defaultBlockState());
        for(String predicate:List.of("all","none","fluid")) {
            var m=queries.level.findBlocksIn(BlockPos.ZERO,new BlockPos(1,0,0)).filterState(filter(predicate));
            var answers=new ArrayList<String>(List.of(""+m.anyMatched(),""+m.noneMatched(),""+m.allMatched()));
            for(int n:new int[]{-1,0,1,2,Integer.MAX_VALUE}){answers.add(""+m.atLeastMatched(n));answers.add(""+m.atMostMatched(n));}
            System.out.println("queries\t"+predicate+"\t"+String.join(",",answers));
        }
        for(boolean box:List.of(false,true)) {
            var f=new BlockScanVectors(registries,"0,0;1,0;2,0");f.set(BlockPos.ZERO,Blocks.WATER.defaultBlockState());
            var calls=new java.util.concurrent.atomic.AtomicInteger();
            Predicate<BlockState> reject=s->{calls.incrementAndGet();return false;};
            BlockMatcher matcher=box?f.level.findBlocksIn(BlockPos.ZERO,new BlockPos(32,16,0)).filterState(reject):f.level.findBlocksIn(ps).filterState(reject);
            matcher.forEach((p,s)->{throw new AssertionError("pruned section visited");});
            System.out.println("prune\t"+box+"\t"+calls.get());
        }
        var first=queries.level.findBlocksIn(List.of(new BlockPos(1,0,0),BlockPos.ZERO,BlockPos.ZERO)).filterState(filter("fluid"));
        System.out.println("first\t"+p(first.findFirst().orElseThrow())+"\t"+p(first.findFirst((p,s)->p.getX()==0).orElseThrow()));
        for(boolean box:List.of(false,true)) {
            var f=new BlockScanVectors(registries,"0,0");var found=new ArrayList<BlockPos>();
            var matcher=box?f.level.findBlocksInBoxByManhattanDistance(BlockPos.ZERO,1,1):f.level.findBlocksInManhattan(BlockPos.ZERO,1);
            matcher.filterPos(p->p.getX()>=0&&p.getZ()>=0).forEach((p,s)->found.add(p.immutable()));
            System.out.println("manhattan\t"+box+"\t"+positions(found));
        }
        for(String loaded:List.of("0,0","0,0;1,0")){
            var f=new BlockScanVectors(registries,loaded);f.set(new BlockPos(15,0,0),Blocks.WATER.defaultBlockState());
            var ids=f.level.getBlockStatesIfLoaded(new AABB(15,0,0,16,1,0)).map(s->""+Block.getId(s)).collect(Collectors.joining(","));
            System.out.println("if-loaded\t"+loaded+"\t"+ids+"\t"+String.join(";",f.reads));
        }
    }
}
