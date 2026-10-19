import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.tags.BlockTags;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.DirectionalPlaceContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.phys.shapes.*;

/** Native connections and wall coverage. Static registry states are grouped by
 * the native predicates they exercise; dynamic block-entity shapes are separate
 * required dependencies, never substituted with empty-world cache baselines. */
public final class ConnectedVectors extends MutationFixture {
    static final BlockPos POS=new BlockPos(8,64,8);
    static final Direction[] SIDES={Direction.NORTH,Direction.EAST,Direction.SOUTH,Direction.WEST};
    ConnectedVectors(RegistryAccess registries){super(registries);}
    @Override public long getGameTime(){return 0;}
    // Atlas setup replaces static states with ordinary block entities. Their
    // unrelated removal notifications have no listeners in this query fixture.
    @Override public void gameEvent(Holder event,net.minecraft.world.phys.Vec3 pos,net.minecraft.world.level.gameevent.GameEvent.Context context){}
    @Override public void levelEvent(net.minecraft.world.entity.Entity source,int event,BlockPos pos,int data){}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,double x,double y,double z,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,net.minecraft.world.entity.Entity entity,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){}
    void raw(BlockPos pos,BlockState state){getChunkAt(pos).setBlockState(pos,state,816);}
    void clear(){for(var direction:Direction.values())raw(POS.relative(direction),Blocks.AIR.defaultBlockState());}
    static int id(BlockState state){return Block.getId(state);}
    static void emit(String text){System.out.println("BUILDING\t"+text);}
    static void run()throws Exception{
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new ConnectedVectors(registries);
        var wall=(WallBlock)Blocks.COBBLESTONE_WALL;var oak=(FenceBlock)Blocks.OAK_FENCE;
        var nether=(FenceBlock)Blocks.NETHER_BRICK_FENCE;var bars=(IronBarsBlock)Blocks.IRON_BARS;
        var wallConnects=WallBlock.class.getDeclaredMethod("connectsTo",BlockState.class,boolean.class,Direction.class);wallConnects.setAccessible(true);
        var probesField=WallBlock.class.getDeclaredField("TEST_SHAPES_WALL");probesField.setAccessible(true);
        @SuppressWarnings("unchecked") var probes=(Map<Direction,VoxelShape>)probesField.get(null);
        var postField=WallBlock.class.getDeclaredField("TEST_SHAPE_POST");postField.setAccessible(true);
        var post=(VoxelShape)postField.get(null);
        var neighborAtlas=new LinkedHashMap<String,BlockState>();var topAtlas=new LinkedHashMap<String,BlockState>();
        for(var block:BuiltInRegistries.BLOCK)if(!block.hasDynamicShape())for(var state:block.getStateDefinition().getPossibleStates()){
            int signature=0;
            for(var side:SIDES){
                var opposite=side.getOpposite();boolean solid=state.isFaceSturdy(level,POS.relative(side),opposite);
                int bits=(solid?1:0)|(oak.connectsTo(state,solid,opposite)?2:0)|(nether.connectsTo(state,solid,opposite)?4:0)
                    |(bars.attachsTo(state,solid)?8:0)|((boolean)wallConnects.invoke(wall,state,solid,opposite)?16:0);
                signature=(signature<<5)|bits;
            }
            neighborAtlas.putIfAbsent(block.getClass().getName()+":"+signature,state);
            var face=state.getCollisionShape(level,POS.above()).getFaceShape(Direction.DOWN);int covered=0;
            for(int i=0;i<SIDES.length;i++)if(!Shapes.joinIsNotEmpty(probes.get(SIDES[i]),face,BooleanOp.ONLY_FIRST))covered|=1<<i;
            if(!Shapes.joinIsNotEmpty(post,face,BooleanOp.ONLY_FIRST))covered|=16;
            var key=covered+":"+state.is(BlockTags.WALL_POST_OVERRIDE)+":"+(block instanceof WallBlock&&state.getValue(WallBlock.UP));
            topAtlas.putIfAbsent(key,state);
        }
        // Each explicit native exception identity gets its own case even when
        // its static predicate signature equals another registered block's.
        for(var block:List.of(Blocks.BARRIER,Blocks.CARVED_PUMPKIN,Blocks.JACK_O_LANTERN,Blocks.MELON,Blocks.PUMPKIN))neighborAtlas.put("exception:"+BuiltInRegistries.BLOCK.getId(block),block.defaultBlockState());
        var fluids=List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8));
        var representatives=Set.of(Blocks.COBBLESTONE_WALL,Blocks.OAK_FENCE,Blocks.NETHER_BRICK_FENCE,Blocks.IRON_BARS,Blocks.GLASS_PANE,Blocks.COPPER_BARS.weathering().unaffected(),Blocks.COPPER_BARS.waxed().unaffected());
        int blocks=0;
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof WallBlock||block instanceof FenceBlock||block instanceof IronBarsBlock){
            blocks++;
            var tops=block instanceof WallBlock?List.copyOf(topAtlas.values()):List.of(Blocks.AIR.defaultBlockState());
            for(int mask=0;mask<16;mask++)for(var fluid:fluids)for(var top:tops){
                level.clear();level.raw(POS,fluid);level.raw(POS.above(),top);
                for(int i=0;i<SIDES.length;i++)if((mask&(1<<i))!=0)level.raw(POS.relative(SIDES[i]),Blocks.STONE.defaultBlockState());
                var context=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
                emit("place\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+mask+"\t"+id(fluid)+"\t"+id(top)+"\t"+id(block.getStateForPlacement(context)));
            }
            var states=representatives.contains(block)?block.getStateDefinition().getPossibleStates():List.of(block.defaultBlockState(),block.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));
            for(var state:states)for(var direction:Direction.values())for(var neighbor:List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState()))for(var top:List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState())){
                level.clear();level.raw(POS,state);var at=POS.relative(direction);
                // updateShape receives the captured neighbor, which may differ
                // from the live block after an earlier synchronous callback.
                level.raw(at,neighbor.isAir()?Blocks.STONE.defaultBlockState():Blocks.AIR.defaultBlockState());level.raw(POS.above(),top);level.trace.clear();
                var result=state.updateShape(level,level,POS,direction,at,neighbor,level.random);
                var ticks=level.trace.stream().filter(s->s.startsWith("fluid:")).toList();
                emit("update\t"+id(state)+"\t"+direction.get3DDataValue()+"\t"+id(neighbor)+"\t"+id(top)+"\t"+id(result)+"\t"+String.join(",",ticks));
            }
        }
        if(blocks!=72)throw new AssertionError("Expected72 blocks, got "+blocks);
        for(var block:List.of(Blocks.OAK_FENCE,Blocks.NETHER_BRICK_FENCE,Blocks.IRON_BARS,Blocks.COBBLESTONE_WALL))for(var neighbor:neighborAtlas.values())for(var direction:SIDES)for(var fluid:fluids){
            level.clear();level.raw(POS,fluid);level.raw(POS.relative(direction),neighbor);
            var context=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
            emit("neighbor\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+id(neighbor)+"\t"+direction.get3DDataValue()+"\t"+id(fluid)+"\t"+id(block.getStateForPlacement(context)));
        }
        System.out.println("Native atlas: "+neighborAtlas.size()+" neighbor footprints, "+topAtlas.size()+" wall coverage masks");
    }
}

final class ConnectedBootstrap {
    public static void main(String[] args)throws Exception{
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        ConnectedVectors.run();
    }
}
