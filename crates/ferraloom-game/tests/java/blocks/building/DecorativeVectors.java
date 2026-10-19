import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.phys.*;

/** Native same-block stacking, replacement and placement sound observations. */
public final class DecorativeVectors extends AttachmentVectors {
    final List<String> sounds=new ArrayList<>();
    DecorativeVectors(RegistryAccess r){super(r);}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,double x,double y,double z,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){
        sounds.add(BuiltInRegistries.SOUND_EVENT.getId((net.minecraft.sounds.SoundEvent)sound.value())+":"+source.ordinal()+":"+Float.floatToRawIntBits(volume)+":"+Float.floatToRawIntBits(pitch)+":"+seed+":"+(int)(x*8)+":"+(int)(y*8)+":"+(int)(z*8));
    }
    static final class Context extends BlockPlaceContext {
        Context(DecorativeVectors level,Actor actor,ItemStack stack,Direction face){super(level,actor,net.minecraft.world.InteractionHand.MAIN_HAND,stack,new BlockHitResult(Vec3.atCenterOf(POS),face,POS,false));}
    }
    static void run()throws Exception{
        var r=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),r.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new DecorativeVectors(r);var actor=new Actor(level);
        var owners=Set.of(CandleBlock.class,TurtleEggBlock.class,CarvedPumpkinBlock.class,EndPortalFrameBlock.class,BarrierBlock.class,DriedGhastBlock.class);
        var targets=new ArrayList<Block>();
        for(var b:BuiltInRegistries.BLOCK)if(owners.contains(b.getClass().getMethod("getStateForPlacement",BlockPlaceContext.class).getDeclaringClass()))targets.add(b);
        if(targets.size()!=23)throw new AssertionError(targets.size());
        var fluids=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,1),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,7),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,15),Blocks.LAVA.defaultBlockState(),Blocks.LAVA.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.IRON_CHAIN.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));
        for(var b:targets){
            var replaced=new ArrayList<>(fluids);
            if(b instanceof CandleBlock||b instanceof TurtleEggBlock)replaced.addAll(b.getStateDefinition().getPossibleStates());
            for(var old:replaced)for(var direction:Direction.values())for(var face:Direction.values()){
                level.raw(POS,old);var c=new DirectionalPlaceContext(level,POS,direction,ItemStack.EMPTY,face);
                emit("place\t"+BuiltInRegistries.BLOCK.getId(b)+"\t"+id(old)+"\t"+direction.get3DDataValue()+"\t"+face.get3DDataValue()+"\t"+id(b.getStateForPlacement(c)));
            }
            if(b instanceof CandleBlock||b instanceof TurtleEggBlock)for(var state:b.getStateDefinition().getPossibleStates())for(var face:Direction.values())for(int secondary=0;secondary<2;secondary++)for(int kind=0;kind<4;kind++){
                level.mask(0);level.raw(POS,state);actor.setShiftKeyDown(secondary==1);
                var stack=switch(kind){case 1->new ItemStack(b);case 2->new ItemStack(b.asItem(),0);case 3->new ItemStack(Blocks.STONE);default->ItemStack.EMPTY;};
                var c=new Context(level,actor,stack,face);
                emit("replace\t"+id(state)+"\t"+face.get3DDataValue()+"\t"+secondary+"\t"+kind+"\t"+c.replacingClickedOnBlock()+"\t"+state.canBeReplaced(c)+"\t"+id(b.getStateForPlacement(c)));
            }
            if(b instanceof CandleBlock||b instanceof BarrierBlock||b instanceof DriedGhastBlock)for(var state:b.getStateDefinition().getPossibleStates())for(var side:Direction.values()){
                level.raw(POS,state);level.trace.clear();var at=POS.relative(side);var neighbor=level.getBlockState(at);
                emit("update\t"+id(state)+"\t"+side.get3DDataValue()+"\t"+id(neighbor)+"\t"+id(state.updateShape(level,level,POS,side,at,neighbor,level.random))+"\t"+String.join(",",level.trace.stream().filter(s->s.startsWith("fluid:")).toList()));
            }
            if(b instanceof CandleBlock)for(var state:b.getStateDefinition().getPossibleStates())for(var below:List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.OAK_FENCE.defaultBlockState(),Blocks.IRON_BARS.defaultBlockState())){
                level.raw(POS.below(),below);emit("survive\t"+id(state)+"\t"+id(below)+"\t"+state.canSurvive(level,POS));
            }
        }
        var field=net.minecraft.world.level.Level.class.getDeclaredField("soundSeedGenerator");field.setAccessible(true);
        for(var state:Blocks.DRIED_GHAST.getStateDefinition().getPossibleStates()){
            ((net.minecraft.util.RandomSource)field.get(level)).setSeed(123);level.sounds.clear();
            Blocks.DRIED_GHAST.setPlacedBy(level,POS,state,null,ItemStack.EMPTY);
            Blocks.DRIED_GHAST.setPlacedBy(level,POS,state,null,ItemStack.EMPTY);
            emit("sound\t"+id(state)+"\t"+String.join(";",level.sounds));
        }
    }
}
final class DecorativeBootstrap {
    public static void main(String[] args)throws Exception{var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);DecorativeVectors.run();}
}
