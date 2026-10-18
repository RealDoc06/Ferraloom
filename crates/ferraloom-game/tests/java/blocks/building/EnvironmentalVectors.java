import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.tags.BlockTags;
import net.minecraft.world.attribute.*;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.DirectionalPlaceContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;

/** Real log-tag/environment and geyser-source state transitions; origin is air
 * or a canonical wrong-type comparator, so no sulfur receiver is invented. */
public final class EnvironmentalVectors extends AttachmentVectors {
    EnvironmentAttributeSystem attributes=EnvironmentAttributeSystem.builder().build();
    EnvironmentalVectors(RegistryAccess r){super(r);}
    @Override public EnvironmentAttributeSystem environmentAttributes(){return attributes;}
    static void run()throws Exception{
        var r=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),r.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new EnvironmentalVectors(r);
        var logs=new ArrayList<BlockState>(List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState()));
        logs.addAll(Blocks.OAK_LOG.getStateDefinition().getPossibleStates());
        for(var b:BuiltInRegistries.BLOCK)for(var s:b.getStateDefinition().getPossibleStates())if(s.is(BlockTags.PALE_OAK_LOGS))logs.add(s);
        for(int active=0;active<2;active++){
            final boolean awake=active==1;level.attributes=EnvironmentAttributeSystem.builder().addConstantLayer(EnvironmentAttributes.CREAKING_ACTIVE,ignored->awake).build();
            for(var face:Direction.values())for(var positive:logs)for(var negative:logs){
                level.mask(0);level.raw(POS,Blocks.AIR.defaultBlockState());var sides=face.getAxis().getDirections();
                level.raw(POS.relative(sides[0]),positive);level.raw(POS.relative(sides[1]),negative);
                var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,face);
                emit("heart\t"+active+"\t"+face.get3DDataValue()+"\t"+id(positive)+"\t"+id(negative)+"\t"+id(Blocks.CREAKING_HEART.getStateForPlacement(c)));
            }
        }
        for(var state:Blocks.CREAKING_HEART.getStateDefinition().getPossibleStates())for(var side:Direction.values()){
            level.raw(POS,state);level.trace.clear();var at=POS.relative(side);var neighbor=level.getBlockState(at);
            emit("heart-update\t"+id(state)+"\t"+side.get3DDataValue()+"\t"+id(neighbor)+"\t"+id(state.updateShape(level,level,POS,side,at,neighbor,level.random))+"\t"+String.join(",",level.trace.stream().filter(s->s.startsWith("schedule:")).toList()));
        }
        var belowAtlas=new LinkedHashMap<String,BlockState>();
        for(var b:BuiltInRegistries.BLOCK)for(var state:b.getStateDefinition().getPossibleStates()){
            var fluid=state.getFluidState();var key=state.is(BlockTags.CAUSES_CONTINUOUS_GEYSER_ERUPTIONS)+":"+state.is(BlockTags.CAUSES_PERIODIC_GEYSER_ERUPTIONS)+":"+fluid.isEmpty()+":"+fluid.isSource();belowAtlas.putIfAbsent(key,state);
        }
        var above=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,1),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.IRON_CHAIN.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true),Blocks.LAVA.defaultBlockState());
        for(var top:above)for(var base:belowAtlas.values())for(var origin:List.of(Blocks.AIR.defaultBlockState(),Blocks.COMPARATOR.defaultBlockState())){
            level.mask(0);level.raw(POS,origin);level.raw(POS.above(),top);level.raw(POS.below(),base);
            var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
            emit("sulfur\t"+id(top)+"\t"+id(base)+"\t"+id(origin)+"\t"+id(Blocks.POTENT_SULFUR.getStateForPlacement(c)));
            for(var state:Blocks.POTENT_SULFUR.getStateDefinition().getPossibleStates())for(var side:Direction.values()){
                level.trace.clear();
                var result=state.updateShape(level,level,POS,side,POS.relative(side),Blocks.STONE.defaultBlockState(),level.random);
                emit("sulfur-update\t"+id(state)+"\t"+id(top)+"\t"+id(base)+"\t"+id(origin)+"\t"+side.get3DDataValue()+"\t"+id(result));
                if(!level.trace.isEmpty())throw new AssertionError(level.trace);
            }
        }
        System.out.println("Native environment atlas: "+logs.size()+" log states, "+belowAtlas.size()+" geyser sources");
    }
}
final class EnvironmentalBootstrap {
    public static void main(String[] args)throws Exception{var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);EnvironmentalVectors.run();}
}
