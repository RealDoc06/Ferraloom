import com.google.gson.*;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.pathfinder.*;

/** Native classification and cache calls. Context overrides only the cache
 * attachment that normally comes from ServerLevel; no classification is mocked. */
public final class PathTypeCacheVectors {
    static final Gson G=new GsonBuilder().setPrettyPrinting().create();
    static final JsonArray steps=new JsonArray();
    static final List<BlockPos> reads=new ArrayList<>();
    static final class Probe implements BlockGetter {
        BlockState state;
        public BlockState getBlockState(BlockPos pos) {return state;}
        public FluidState getFluidState(BlockPos pos) {throw new UnsupportedOperationException("fluid query");}
        public BlockEntity getBlockEntity(BlockPos pos) {throw new UnsupportedOperationException("block entity query");}
        public int getHeight() {return 384;}
        public int getMinY() {return -64;}
    }
    static final class World extends NeighborFixture {
        World(RegistryAccess registry) {super(registry,512);}
        public BlockState getBlockState(BlockPos pos) {reads.add(pos.immutable());return blocks.getOrDefault(pos,Blocks.AIR.defaultBlockState());}
        public FluidState getFluidState(BlockPos pos) {return getBlockState(pos).getFluidState();}
    }
    static final class Context extends PathfindingContext {
        final PathTypeCache cache=new PathTypeCache();
        Context(World level) {super(level,new Cow(EntityTypes.COW,level));}
        public PathType getPathTypeFromState(int x,int y,int z) {return cache.getOrCompute(level(),new BlockPos(x,y,z));}
    }
    static JsonArray pos(BlockPos p) {var a=new JsonArray();a.add(p.getX());a.add(p.getY());a.add(p.getZ());return a;}
    static void step(String op,World level,Context context,BlockPos p,BlockState state) {
        reads.clear();var row=new JsonObject();row.addProperty("op",op);row.add("pos",pos(p));
        if(state!=null) row.addProperty("state",Block.getId(state));
        switch(op) {
            case "set"->level.blocks.put(p,state);
            case "invalidate"->context.cache.invalidate(p);
            case "raw"->row.addProperty("type",context.getPathTypeFromState(p.getX(),p.getY(),p.getZ()).ordinal());
            case "static"->row.addProperty("type",WalkNodeEvaluator.getPathTypeStatic(context,p.mutable()).ordinal());
            default->throw new AssertionError(op);
        }
        var trace=new JsonArray();for(var read:reads) trace.add(pos(read));row.add("reads",trace);steps.add(row);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var registries=NeighborVectors.load();
        var method=WalkNodeEvaluator.class.getDeclaredMethod("getPathTypeFromState",BlockGetter.class,BlockPos.class);method.setAccessible(true);
        var probe=new Probe();int states=0;
        try(var out=Files.newBufferedWriter(Path.of(args[0]))) {
            for(var state:Block.BLOCK_STATE_REGISTRY) {
                probe.state=state;
                var type=(PathType)method.invoke(null,probe,BlockPos.ZERO);
                out.write(Block.getId(state)+"\t"+type.ordinal()+"\n");states++;
            }
        }
        var world=new World(registries);var context=new Context(world);
        var center=new BlockPos(8,5,8);var hazard=center.offset(-1,-1,-1);
        step("set",world,context,center.below(),Blocks.STONE.defaultBlockState());
        step("static",world,context,center,null);
        step("set",world,context,hazard,Blocks.WATER.defaultBlockState());
        step("static",world,context,center,null);
        step("invalidate",world,context,center,null);
        step("static",world,context,center,null);
        step("invalidate",world,context,hazard,null);
        step("static",world,context,center,null);
        step("set",world,context,hazard,Blocks.LAVA.defaultBlockState());
        step("static",world,context,center,null);
        step("invalidate",world,context,hazard,null);
        step("static",world,context,center,null);
        step("set",world,context,center.below(),Blocks.POWDER_SNOW.defaultBlockState());
        step("static",world,context,center,null);
        step("invalidate",world,context,center.below(),null);
        step("static",world,context,center,null);
        var zero=BlockPos.ZERO;
        step("set",world,context,zero,Blocks.STONE.defaultBlockState());step("raw",world,context,zero,null);
        BlockPos collision=null;
        for(int x=1;x<1000000;x++) {var p=new BlockPos(x,0,0);if(((int)it.unimi.dsi.fastutil.HashCommon.mix(p.asLong())&4095)==0){collision=p;break;}}
        if(collision==null)throw new AssertionError("no collision");
        step("invalidate",world,context,collision,null);step("raw",world,context,zero,null);
        step("raw",world,context,new BlockPos(0,4096,0),null);
        step("raw",world,context,collision,null);step("set",world,context,zero,Blocks.AIR.defaultBlockState());step("raw",world,context,zero,null);
        var result=new JsonObject();result.addProperty("min_y",world.getMinY());result.add("steps",steps);
        Files.writeString(Path.of(args[1]),G.toJson(result)+"\n");
        var observed=new ArrayList<Integer>();for(var e:steps)if(e.getAsJsonObject().has("type"))observed.add(e.getAsJsonObject().get("type").getAsInt());
        if(!observed.subList(0,8).equals(List.of(PathType.WALKABLE.ordinal(),PathType.WALKABLE.ordinal(),PathType.WALKABLE.ordinal(),PathType.WATER_BORDER.ordinal(),PathType.WATER_BORDER.ordinal(),PathType.FIRE_IN_NEIGHBOR.ordinal(),PathType.FIRE_IN_NEIGHBOR.ordinal(),PathType.ON_TOP_OF_POWDER_SNOW.ordinal())))throw new AssertionError(observed);
        System.out.println("PASS "+states+" native raw types; "+steps.size()+" cache/neighbor steps");
    }
}
