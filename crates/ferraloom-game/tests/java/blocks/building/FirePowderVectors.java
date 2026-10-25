import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.DirectionalPlaceContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;

/** Native fire/soul-fire placement distinction and powder liquid-contact paths. */
public final class FirePowderVectors extends AttachmentVectors {
    FirePowderVectors(RegistryAccess r){super(r);}
    void neighbors(int mask,BlockState neighbor){for(var side:Direction.values())raw(POS.relative(side),(mask&(1<<side.get3DDataValue()))==0?Blocks.AIR.defaultBlockState():neighbor);}
    static void run()throws Exception{
        var r=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),r.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new FirePowderVectors(r);
        var bases=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.OAK_PLANKS.defaultBlockState(),Blocks.OAK_FENCE.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true),Blocks.SOUL_SOIL.defaultBlockState(),Blocks.SOUL_SAND.defaultBlockState(),Blocks.NETHER_BRICK_FENCE.defaultBlockState());
        for(var block:List.of(Blocks.FIRE,Blocks.SOUL_FIRE))for(int mask=0;mask<64;mask++)for(var base:bases){
            level.neighbors(mask,Blocks.OAK_PLANKS.defaultBlockState());level.raw(POS.below(),base);level.raw(POS,Blocks.AIR.defaultBlockState());
            var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
            emit("fire-place\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+mask+"\t"+id(base)+"\t"+id(block.getStateForPlacement(c))+"\t"+block.defaultBlockState().canSurvive(level,POS));
        }
        for(var block:List.of(Blocks.FIRE,Blocks.SOUL_FIRE))for(var state:block.getStateDefinition().getPossibleStates())for(int variant=0;variant<7;variant++)for(var side:Direction.values()){
            int mask=variant%2==0?0:63;var base=bases.get(variant);
            level.neighbors(mask,Blocks.OAK_PLANKS.defaultBlockState());level.raw(POS.below(),base);level.raw(POS,state);level.trace.clear();
            var actual=state.updateShape(level,level,POS,side,POS.relative(side),Blocks.AIR.defaultBlockState(),level.random);
            emit("fire-update\t"+id(state)+"\t"+mask+"\t"+id(base)+"\t"+side.get3DDataValue()+"\t"+id(actual));
            if(!level.trace.isEmpty())throw new AssertionError(level.trace);
        }
        var atlas=new LinkedHashMap<Integer,BlockState>();
        for(var b:BuiltInRegistries.BLOCK)if(!b.hasDynamicShape())for(var state:b.getStateDefinition().getPossibleStates())if(state.getFluidState().is(FluidTags.WATER)){
            int bits=0;for(var side:Direction.values())if(state.isFaceSturdy(level,POS,side))bits|=1<<side.get3DDataValue();atlas.putIfAbsent(bits,state);
        }
        int blocks=0;
        for(var b:BuiltInRegistries.BLOCK)if(b instanceof ConcretePowderBlock){
            blocks++;
            for(var old:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.OAK_FENCE.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true),b.defaultBlockState()))for(int mask=0;mask<64;mask++){
                level.neighbors(mask,Blocks.WATER.defaultBlockState());level.raw(POS,old);
                var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
                emit("powder-place\t"+BuiltInRegistries.BLOCK.getId(b)+"\t"+id(old)+"\t"+mask+"\t"+id(b.getStateForPlacement(c)));
            }
            for(var neighbor:atlas.values())for(var side:Direction.values()){
                level.mask(0);level.raw(POS,Blocks.AIR.defaultBlockState());level.raw(POS.relative(side),neighbor);
                var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
                emit("powder-neighbor\t"+BuiltInRegistries.BLOCK.getId(b)+"\t"+id(neighbor)+"\t"+side.get3DDataValue()+"\t"+id(b.getStateForPlacement(c)));
            }
            for(var live:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),b.defaultBlockState()))for(int mask=0;mask<64;mask++)for(var side:Direction.values()){
                level.neighbors(mask,Blocks.WATER.defaultBlockState());level.raw(POS,live);level.trace.clear();
                var actual=b.defaultBlockState().updateShape(level,level,POS,side,POS.relative(side),Blocks.STONE.defaultBlockState(),level.random);
                emit("powder-update\t"+id(b.defaultBlockState())+"\t"+id(live)+"\t"+mask+"\t"+side.get3DDataValue()+"\t"+id(actual)+"\t"+String.join(",",level.trace.stream().filter(s->s.startsWith("schedule:")).toList()));
            }
        }
        if(blocks!=16)throw new AssertionError(blocks);
        System.out.println("Native powder water footprints: "+atlas.size());
    }
}
final class FirePowderBootstrap {
    public static void main(String[] args)throws Exception{var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);FirePowderVectors.run();}
}
