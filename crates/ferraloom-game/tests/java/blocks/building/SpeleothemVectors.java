import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.ticks.*;

/** Native shared-tag versus same-block thickness rules and exact LevelTicks
 * deduplication, including pre-existing downward-tip scheduled ticks. */
public final class SpeleothemVectors extends AttachmentVectors {
    final LevelTicks<Block> ticks=new LevelTicks<>(key->true);
    LevelChunkTicks<Block> container;
    long order;
    SpeleothemVectors(RegistryAccess r){super(r);reset();}
    void reset(){var chunk=new net.minecraft.world.level.ChunkPos(0,0);ticks.removeContainer(chunk);container=new LevelChunkTicks<>();ticks.addContainer(chunk,container);order=0;}
    @Override public LevelTickAccess<Block> getBlockTicks(){return ticks;}
    @Override public void scheduleTick(BlockPos pos,Block block,int delay){ticks.schedule(new ScheduledTick<>(block,pos,delay,TickPriority.NORMAL,order++));}
    String queued(){
        var result=new ArrayList<String>();for(var tick:container.pack(0))result.add("schedule:"+tick.delay());result.addAll(trace.stream().filter(s->s.startsWith("fluid:")).toList());return String.join(",",result);
    }
    static final class Context extends BlockPlaceContext {
        Context(SpeleothemVectors level,Actor actor){super(level,actor,net.minecraft.world.InteractionHand.MAIN_HAND,ItemStack.EMPTY,new net.minecraft.world.phys.BlockHitResult(net.minecraft.world.phys.Vec3.atCenterOf(POS),Direction.UP,POS,false));}
    }
    static void run()throws Exception{
        var r=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),r.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new SpeleothemVectors(r);var actor=new Actor(level);
        var neighbors=new ArrayList<BlockState>(List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.OAK_SLAB.defaultBlockState(),Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP)));
        for(var b:List.of(Blocks.POINTED_DRIPSTONE,Blocks.SULFUR_SPIKE))for(var state:b.getStateDefinition().getPossibleStates())if(!state.getValue(BlockStateProperties.WATERLOGGED))neighbors.add(state);
        var fluids=List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8));
        for(var block:List.of(Blocks.POINTED_DRIPSTONE,Blocks.SULFUR_SPIKE))for(var below:neighbors)for(var above:neighbors)for(int pitch:new int[]{-45,45})for(int secondary=0;secondary<2;secondary++)for(var fluid:fluids){
            level.raw(POS,fluid);level.raw(POS.below(),below);level.raw(POS.above(),above);actor.setXRot(pitch);actor.setShiftKeyDown(secondary==1);
            emit("place\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+id(below)+"\t"+id(above)+"\t"+pitch+"\t"+secondary+"\t"+id(fluid)+"\t"+id(block.getStateForPlacement(new Context(level,actor))));
        }
        for(var block:List.of(Blocks.POINTED_DRIPSTONE,Blocks.SULFUR_SPIKE))for(var state:block.getStateDefinition().getPossibleStates())for(var below:neighbors)for(var above:neighbors)for(var side:Direction.values())for(int scheduled=0;scheduled<2;scheduled++){
            // Horizontal updates never inspect the neighbor geometry. One pair
            // covers each state/direction/tick mask without redundant products.
            if(side.getAxis().isHorizontal()&&(below!=neighbors.getFirst()||above!=neighbors.getFirst()))continue;
            level.raw(POS,state);level.raw(POS.below(),below);level.raw(POS.above(),above);level.reset();
            if(scheduled==1)level.scheduleTick(POS,block,7);level.trace.clear();
            var result=state.updateShape(level,level,POS,side,POS.relative(side),Blocks.AIR.defaultBlockState(),level.random);
            emit("update\t"+id(state)+"\t"+id(below)+"\t"+id(above)+"\t"+side.get3DDataValue()+"\t"+scheduled+"\t"+id(result)+"\t"+state.canSurvive(level,POS)+"\t"+level.queued());
        }
        for(var block:List.of(Blocks.POINTED_DRIPSTONE,Blocks.SULFUR_SPIKE))for(var state:block.getStateDefinition().getPossibleStates())for(var pos:List.of(BlockPos.ZERO,POS,new BlockPos(-1,319,-1),new BlockPos(1,-64,1),new BlockPos(12345,0,-6789),new BlockPos(30000000,64,-30000000),new BlockPos(-30000000,64,30000000))){
            var b=state.getShape(level,pos).bounds();
            emit("shape\t"+id(state)+"\t"+pos.getX()+"\t"+pos.getY()+"\t"+pos.getZ()+"\t"+String.join(",",java.util.stream.DoubleStream.of(b.minX,b.minY,b.minZ,b.maxX,b.maxY,b.maxZ).mapToObj(v->Long.toString(Double.doubleToRawLongBits(v))).toList()));
        }
    }
}
final class SpeleothemBootstrap {
    public static void main(String[] args)throws Exception{var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);SpeleothemVectors.run();}
}
