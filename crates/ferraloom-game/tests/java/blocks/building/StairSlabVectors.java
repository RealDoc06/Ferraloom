import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.phys.*;

/** Native stair/slab queries, including every placement-input branch and every
 * corner mask. The private native predicates are exercised on actual chunks. */
public final class StairSlabVectors extends MutationFixture {
    static final BlockPos POS = new BlockPos(8,64,8);
    static final Direction[] HORIZONTAL={Direction.NORTH,Direction.EAST,Direction.SOUTH,Direction.WEST};
    static final double[] HEIGHTS={64.0,64.5,Math.nextUp(64.5),65.0,Double.NaN};
    StairSlabVectors(RegistryAccess registries) {super(registries);}
    void raw(BlockPos pos,BlockState state){getChunkAt(pos).setBlockState(pos,state,816);}
    void clear(){for(var direction:Direction.values())raw(POS.relative(direction),Blocks.AIR.defaultBlockState());}
    static void emit(String value){System.out.println("BUILDING\t"+value);}
    static int id(BlockState state){return state==null?-1:Block.getId(state);}
    static final class Context extends BlockPlaceContext {
        Context(Level level,ItemStack item,Direction face,double y){
            super(level,null,InteractionHand.MAIN_HAND,item,new BlockHitResult(new Vec3(8.5,y,8.5),face,POS,false));
        }
    }
    static void run() throws Exception {
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new StairSlabVectors(registries);level.getChunkAt(POS).setLoaded(true);
        var replaced=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),
            Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,1),
            Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.LAVA.defaultBlockState(),
            Blocks.IRON_CHAIN.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));
        int blocks=0;
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof StairBlock||block instanceof SlabBlock){
            blocks++;var olds=new ArrayList<BlockState>(replaced);
            if(block instanceof SlabBlock)olds.addAll(block.getStateDefinition().getPossibleStates());
            level.clear();
            for(var direction:Direction.values())for(var face:Direction.values())for(var old:olds){
                level.raw(POS,old);
                var context=new DirectionalPlaceContext(level,POS,direction,ItemStack.EMPTY,face);
                emit("place\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+direction.get3DDataValue()+"\t"+face.get3DDataValue()+"\t"+id(old)+"\t"+id(block.getStateForPlacement(context)));
            }
            for(var face:Direction.values())for(int h=0;h<HEIGHTS.length;h++)for(var old:List.of(replaced.get(0),replaced.get(2),replaced.get(4))){
                level.raw(POS,old);var context=new Context(level,ItemStack.EMPTY,face,HEIGHTS[h]);
                emit("height\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+face.get3DDataValue()+"\t"+h+"\t"+id(old)+"\t"+id(block.getStateForPlacement(context)));
            }
            for(var state:block.getStateDefinition().getPossibleStates()){
                if(block instanceof SlabBlock)for(var face:Direction.values())for(int h=0;h<HEIGHTS.length;h++)for(int held=0;held<4;held++){
                    level.clear();level.raw(POS,state);
                    var item=switch(held){case 0->ItemStack.EMPTY;case 1->new ItemStack(block.asItem());case 2->new ItemStack(block.asItem(),0);default->new ItemStack(Items.STONE);};
                    var context=new Context(level,item,face,HEIGHTS[h]);
                    emit("replace\t"+id(state)+"\t"+face.get3DDataValue()+"\t"+h+"\t"+held+"\t"+context.replacingClickedOnBlock()+"\t"+state.canBeReplaced(context)+"\t"+id(block.getStateForPlacement(context)));
                }
                for(var direction:Direction.values()){
                    level.clear();level.raw(POS,state);level.trace.clear();var at=POS.relative(direction);
                    var result=state.updateShape(level,level,POS,direction,at,Blocks.AIR.defaultBlockState(),level.random);
                    var ticks=level.trace.stream().filter(s->s.startsWith("fluid:")||s.startsWith("schedule:")).toList();
                    emit("update\t"+id(state)+"\t"+direction.get3DDataValue()+"\t"+id(result)+"\t"+String.join(",",ticks));
                }
                if(block instanceof SlabBlock slab)for(var source:replaced.subList(2,5)){
                    level.clear();level.raw(POS,state);level.trace.clear();var fluid=source.getFluidState();
                    boolean allowed=slab.canPlaceLiquid(null,level,POS,state,fluid.getType());
                    boolean placed=slab.placeLiquid(level,POS,state,fluid);
                    var ticks=level.trace.stream().filter(s->s.startsWith("fluid:")).toList();
                    // Synchronous shape updates may request the same fluid tick
                    // repeatedly; the queue stores only one native type/position.
                    emit("liquid\t"+id(state)+"\t"+id(source)+"\t"+allowed+"\t"+placed+"\t"+id(level.getBlockState(POS))+"\t"+(ticks.isEmpty()?"":"fluid:5"));
                }
            }
        }
        if(blocks!=198)throw new AssertionError("Expected198 blocks, got "+blocks);
        var shape=StairBlock.class.getDeclaredMethod("getStairsShape",BlockState.class,net.minecraft.world.level.BlockGetter.class,BlockPos.class);shape.setAccessible(true);
        var canTake=StairBlock.class.getDeclaredMethod("canTakeShape",BlockState.class,net.minecraft.world.level.BlockGetter.class,BlockPos.class,Direction.class);canTake.setAccessible(true);
        var neighbors=new ArrayList<BlockState>();neighbors.add(Blocks.AIR.defaultBlockState());
        for(var half:Half.values())for(var face:HORIZONTAL)neighbors.add(Blocks.BRICK_STAIRS.defaultBlockState().setValue(StairBlock.HALF,half).setValue(StairBlock.FACING,face));
        for(var state:Blocks.OAK_STAIRS.getStateDefinition().getPossibleStates())if(state.getValue(StairBlock.SHAPE)==StairsShape.STRAIGHT){
            var facing=state.getValue(StairBlock.FACING);
            for(var behind:neighbors)for(var front:neighbors)for(int mask=0;mask<4;mask++){
                level.clear();level.raw(POS,state);level.raw(POS.relative(facing),behind);level.raw(POS.relative(facing.getOpposite()),front);
                if((mask&1)!=0)level.raw(POS.relative(facing.getCounterClockWise()),state);
                if((mask&2)!=0)level.raw(POS.relative(facing.getClockWise()),state);
                var result=state.setValue(StairBlock.SHAPE,(StairsShape)shape.invoke(null,state,level,POS));
                emit("corner\t"+id(state)+"\t"+id(behind)+"\t"+id(front)+"\t"+mask+"\t"+id(result));
            }
            if(!state.getValue(StairBlock.WATERLOGGED)){
                for(var block:BuiltInRegistries.BLOCK)if(block instanceof StairBlock)for(var neighbor:block.getStateDefinition().getPossibleStates()){
                    level.raw(POS.east(),neighbor);
                    emit("take-shape\t"+id(state)+"\t"+id(neighbor)+"\t"+canTake.invoke(null,state,level,POS,Direction.EAST));
                }
                for(var neighbor:List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.OAK_FENCE.defaultBlockState())){
                    level.raw(POS.east(),neighbor);
                    emit("take-shape\t"+id(state)+"\t"+id(neighbor)+"\t"+canTake.invoke(null,state,level,POS,Direction.EAST));
                }
            }
        }
    }
}

final class StairSlabBootstrap {
    public static void main(String[] args) throws Exception {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        StairSlabVectors.run();
    }
}
