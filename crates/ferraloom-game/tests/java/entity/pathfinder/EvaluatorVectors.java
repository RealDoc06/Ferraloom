import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.tags.FluidTags;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.pathfinder.*;
import net.minecraft.world.phys.*;

/** Real native evaluators and PathFinder, with controlled Mob virtual inputs and
 * real LevelChunks captured by PathNavigationRegion. No path types or neighbors
 * are supplied by the fixture. */
public final class EvaluatorVectors {
    static final Gson JSON=new Gson();
    static final class World extends NeighborFixture {
        final Map<ChunkPos,LevelChunk> chunks=new HashMap<>();
        final Map<BlockPos,BlockState> placed=new TreeMap<>(Comparator.comparingLong(BlockPos::asLong));
        final WorldBorderSource source=new WorldBorderSource();
        final net.minecraft.world.level.border.WorldBorder border=new net.minecraft.world.level.border.WorldBorder();
        World(RegistryAccess registries){super(registries,512);}
        final class WorldBorderSource extends ChunkSource {
            final net.minecraft.world.level.lighting.LevelLightEngine light=new net.minecraft.world.level.lighting.LevelLightEngine(this,false,false);
            public BlockGetter getLevel(){return World.this;}
            public ChunkAccess getChunk(int x,int z,ChunkStatus status,boolean load){var p=new ChunkPos(x,z);return load?chunks.computeIfAbsent(p,k->new LevelChunk(World.this,k)):chunks.get(p);}
            public void tick(java.util.function.BooleanSupplier time,boolean tick){}
            public String gatherStats(){return "pathfinder fixture";}
            public int getLoadedChunksCount(){return chunks.size();}
            public net.minecraft.world.level.lighting.LevelLightEngine getLightEngine(){return light;}
            public void onSectionEmptinessChanged(int x,int y,int z,boolean empty){}
        }
        public int getSeaLevel(){return 63;}
        public ChunkSource getChunkSource(){return source;}
        public BlockState getBlockState(BlockPos pos){var chunk=chunks.get(ChunkPos.containing(pos));return chunk==null?Blocks.AIR.defaultBlockState():chunk.getBlockState(pos);}
        public FluidState getFluidState(BlockPos pos){return getBlockState(pos).getFluidState();}
        public net.minecraft.world.level.border.WorldBorder getWorldBorder(){return border;}
        void put(BlockPos pos,BlockState state){getChunkAt(pos).setBlockState(pos,state,816);placed.put(pos.immutable(),state);}
    }
    static final class Probe extends Cow {
        float width,height,step=0.6f;int fall=3,starts,ends;boolean water,floatable,standWater,standLava;
        List<String> writes=new ArrayList<>();
        Probe(World world,float width,float height){super(EntityTypes.COW,world);this.width=width;this.height=height;refreshDimensions();setPos(4.5,1,8.5);setOnGround(true);}
        public EntityDimensions getDefaultDimensions(Pose pose){return width>0?EntityDimensions.scalable(width,height):super.getDefaultDimensions(pose);}
        public boolean isInWater(){return water;}
        public boolean isInFloatableFluid(){return floatable;}
        public boolean canStandOnFluid(FluidState fluid){return (standWater&&fluid.is(FluidTags.WATER))||(standLava&&fluid.is(FluidTags.LAVA));}
        public float maxUpStep(){return step;}
        public int getMaxFallDistance(){return fall;}
        public void onPathfindingStart(){starts++;}
        public void onPathfindingDone(){ends++;}
        public void setPathfindingMalus(PathType type,float value){super.setPathfindingMalus(type,value);if(writes!=null)writes.add(type.ordinal()+":"+bits(value));}
    }
    static JsonArray pos(BlockPos pos){var a=new JsonArray();a.add(pos.getX());a.add(pos.getY());a.add(pos.getZ());return a;}
    static int bits(float f){return Float.floatToRawIntBits(f);}
    static JsonObject node(Node node){var out=new JsonObject();out.add("pos",pos(node.asBlockPos()));out.addProperty("type",node.type.ordinal());out.addProperty("g",bits(node.g));out.addProperty("h",bits(node.h));out.addProperty("f",bits(node.f));out.addProperty("malus",bits(node.costMalus));out.addProperty("walked",bits(node.walkedDistance));out.addProperty("closed",node.closed);return out;}
    static void layout(World world,String name){
        world.getChunk(0,0);
        for(int x=0;x<16;x++)for(int z=0;z<16;z++)world.put(new BlockPos(x,0,z),Blocks.STONE.defaultBlockState());
        BlockState obstacle=switch(name){
            case "slab"->Blocks.STONE_SLAB.defaultBlockState();
            case "top-slab"->Blocks.STONE_SLAB.defaultBlockState().setValue(BlockStateProperties.SLAB_TYPE,SlabType.TOP);
            case "stairs"->Blocks.STONE_STAIRS.defaultBlockState().setValue(BlockStateProperties.HORIZONTAL_FACING,Direction.WEST);
            case "fence"->Blocks.OAK_FENCE.defaultBlockState();
            case "wall"->Blocks.COBBLESTONE_WALL.defaultBlockState();
            case "closed-door"->Blocks.OAK_DOOR.defaultBlockState();
            case "open-door"->Blocks.OAK_DOOR.defaultBlockState().setValue(BlockStateProperties.OPEN,true);
            case "iron-door"->Blocks.IRON_DOOR.defaultBlockState();
            case "water"->Blocks.WATER.defaultBlockState();
            case "lava"->Blocks.LAVA.defaultBlockState();
            case "fire"->Blocks.FIRE.defaultBlockState();
            case "cobweb"->Blocks.COBWEB.defaultBlockState();
            case "berries"->Blocks.SWEET_BERRY_BUSH.defaultBlockState();
            case "cactus"->Blocks.CACTUS.defaultBlockState();
            case "honey"->Blocks.HONEY_BLOCK.defaultBlockState();
            case "powder"->Blocks.POWDER_SNOW.defaultBlockState();
            case "rail"->Blocks.RAIL.defaultBlockState();
            case "trapdoor"->Blocks.OAK_TRAPDOOR.defaultBlockState();
            case "leaves"->Blocks.OAK_LEAVES.defaultBlockState();
            default->Blocks.STONE.defaultBlockState();
        };
        if(name.equals("pool")||name.equals("deep-pool")){
            for(int x=1;x<15;x++)for(int z=1;z<15;z++)for(int y=1;y<=4;y++)world.put(new BlockPos(x,y,z),Blocks.WATER.defaultBlockState());
        }else if(name.equals("ledge")){
            for(int x=6;x<12;x++)for(int z=4;z<13;z++)world.put(new BlockPos(x,0,z),Blocks.AIR.defaultBlockState());
            for(int x=6;x<12;x++)for(int z=4;z<13;z++)world.put(new BlockPos(x,-3,z),Blocks.STONE.defaultBlockState());
        }else if(name.equals("ceiling")){
            for(int x=6;x<12;x++)for(int z=4;z<13;z++)world.put(new BlockPos(x,2,z),Blocks.STONE.defaultBlockState());
        }else if(name.equals("fly-blocked-start")){
            for(int x=3;x<=5;x++)for(int z=7;z<=9;z++)world.put(new BlockPos(x,1,z),Blocks.STONE.defaultBlockState());
        }else if(!name.equals("flat")){
            for(int z=4;z<=12;z++)world.put(new BlockPos(7,1,z),obstacle);
            if(obstacle.getBlock() instanceof DoorBlock)for(int z=4;z<=12;z++)world.put(new BlockPos(7,2,z),obstacle.setValue(BlockStateProperties.DOUBLE_BLOCK_HALF,DoubleBlockHalf.UPPER));
        }
    }
    static JsonObject run(RegistryAccess registries,String kind,String layout,float width,int flags,int budget,int reach,String operation) throws Exception{
        var world=new World(registries);layout(world,layout);
        var mob=new Probe(world,width,width<0.5f?0.4f:1.4f);
        if(layout.equals("pool")||layout.equals("deep-pool")){mob.water=true;mob.floatable=true;mob.setPos(4.5,2,8.5);}
        if(layout.equals("deep-pool")){mob.setPos(4.5,1,8.5);}
        if((flags&16)!=0)mob.setOnGround(false);
        if((flags&32)!=0)mob.step=2;
        if((flags&64)!=0)mob.fall=0;
        if((flags&128)!=0){mob.standWater=true;mob.standLava=true;}
        mob.getRandom().setSeed(12345);
        var region=new PathNavigationRegion(world,new BlockPos(0,-64,0),new BlockPos(15,319,15));
        NodeEvaluator evaluator=switch(kind){case "fly"->new FlyNodeEvaluator();case "swim"->new SwimNodeEvaluator((flags&8)!=0);case "amphibious"->new AmphibiousNodeEvaluator((flags&8)!=0);default->new WalkNodeEvaluator();};
        evaluator.setCanFloat((flags&1)!=0);evaluator.setCanOpenDoors((flags&2)!=0);evaluator.setCanPassDoors((flags&4)==0);evaluator.setCanWalkOverFences((flags&8)!=0);
        var row=new JsonObject();row.addProperty("kind",kind);row.addProperty("layout",layout);row.addProperty("width",width);row.addProperty("height",mob.getBbHeight());row.addProperty("flags",flags);row.addProperty("budget",budget);row.addProperty("reach",reach);row.addProperty("op",operation);row.addProperty("step",mob.step);row.addProperty("fall",mob.fall);row.addProperty("water",mob.water);row.addProperty("floatable",mob.floatable);row.addProperty("stand_water",mob.standWater);row.addProperty("stand_lava",mob.standLava);
        var coordinates=new JsonArray();coordinates.add(mob.getX());coordinates.add(mob.getY());coordinates.add(mob.getZ());row.add("position",coordinates);
        var targets=new HashSet<BlockPos>();targets.add(new BlockPos(12,1,8));if(reach==2)targets.add(new BlockPos(10,1,3));
        var targetInputs=new JsonArray();for(var target:targets)targetInputs.add(pos(target));row.add("targets",targetInputs);
        if(operation.equals("neighbors")){
            evaluator.prepare(region,mob);var start=evaluator.getStart();row.add("start",node(start));var out=new Node[32];int count=evaluator.getNeighbors(out,start);var nodes=new JsonArray();for(int i=0;i<count;i++)nodes.add(node(out[i]));row.add("nodes",nodes);
            var types=new JsonArray();for(int x=4;x<=8;x++)for(int y=0;y<=3;y++)for(int z=7;z<=9;z++){var value=new JsonObject();value.add("pos",pos(new BlockPos(x,y,z)));value.addProperty("type",evaluator.getPathTypeOfMob(new PathfindingContext(region,mob),x,y,z,mob).ordinal());types.add(value);}row.add("types",types);evaluator.done();
        }else{
            var finder=new PathFinder(evaluator,budget);var path=finder.findPath(region,mob,targets,32,reach,1);
            if(path==null)row.add("nodes",JsonNull.INSTANCE);else{row.addProperty("reached",path.canReach());row.addProperty("dist",bits(path.getDistToTarget()));row.add("target",pos(path.getTarget()));var nodes=new JsonArray();for(int i=0;i<path.getNodeCount();i++)nodes.add(node(path.getNode(i)));row.add("nodes",nodes);}
        }
        row.addProperty("starts",mob.starts);row.addProperty("ends",mob.ends);row.add("writes",JSON.toJsonTree(mob.writes));row.addProperty("next_random",mob.getRandom().nextInt());
        var blocks=new JsonArray();for(var entry:world.placed.entrySet()){var block=new JsonObject();block.add("pos",pos(entry.getKey()));block.addProperty("state",Block.getId(entry.getValue()));blocks.add(block);}row.add("blocks",blocks);
        return row;
    }
    public static void main(String[] args)throws Exception{
        var stdout=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(stdout);
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var rows=new JsonArray();
        var layouts=List.of("flat","slab","top-slab","stairs","fence","wall","closed-door","open-door","iron-door","water","lava","fire","cobweb","berries","cactus","honey","powder","rail","trapdoor","leaves","ledge","ceiling","pool","deep-pool","fly-blocked-start");
        for(String kind:List.of("walk","fly","swim","amphibious"))for(String layout:layouts)for(float width:new float[]{0.3f,0.9f,1.4f})for(int flags:new int[]{0,3,12}){
            rows.add(run(registries,kind,layout,width,flags,256,0,"path"));
        }
        for(String kind:List.of("walk","fly","swim","amphibious"))for(String layout:List.of("flat","fence","closed-door","pool","fly-blocked-start"))for(int budget:new int[]{0,1,2,8,32})for(int reach:new int[]{0,2})rows.add(run(registries,kind,layout,0.9f,1,budget,reach,"path"));
        for(String kind:List.of("walk","fly","swim","amphibious"))for(String layout:layouts)for(int flags:new int[]{0,1,2,4,8,16,32,64,128})rows.add(run(registries,kind,layout,0.9f,flags,256,0,"neighbors"));
        Files.writeString(java.nio.file.Path.of(args[0]),JSON.toJson(rows)+"\n");System.out.println(rows.size()+" real PathFinder/evaluator cases captured");
    }
}
