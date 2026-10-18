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

/** Native attachment queries over all six-face support masks, actual player
 * look orders, static registry support footprints and scaffold distances. */
public final class AttachmentVectors extends MutationFixture {
    static final BlockPos POS=new BlockPos(8,64,8);
    static final Direction[] SIDES={Direction.NORTH,Direction.EAST,Direction.SOUTH,Direction.WEST};
    static final Set<Class<?>> OWNERS=Set.of(LanternBlock.class,LadderBlock.class,WallTorchBlock.class,AmethystClusterBlock.class,EndRodBlock.class,FaceAttachedHorizontalDirectionalBlock.class,ScaffoldingBlock.class);
    AttachmentVectors(RegistryAccess r){super(r);}
    @Override public long getGameTime(){return 0;}
    @Override public void gameEvent(Holder event,Vec3 pos,net.minecraft.world.level.gameevent.GameEvent.Context context){}
    @Override public void levelEvent(net.minecraft.world.entity.Entity source,int event,BlockPos pos,int data){}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,double x,double y,double z,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,net.minecraft.world.entity.Entity entity,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){}
    void raw(BlockPos p,BlockState s){getChunkAt(p).setBlockState(p,s,816);}
    void mask(int mask){for(var side:Direction.values())raw(POS.relative(side),(mask&(1<<side.get3DDataValue()))==0?Blocks.AIR.defaultBlockState():Blocks.STONE.defaultBlockState());}
    static void emit(String text){System.out.println("BUILDING\t"+text);}
    static int id(BlockState s){return s==null?-1:Block.getId(s);}
    static final class Actor extends net.minecraft.world.entity.player.Player {
        Actor(net.minecraft.world.level.Level level){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"building"));}
        @Override public net.minecraft.world.level.GameType gameMode(){return net.minecraft.world.level.GameType.SURVIVAL;}
    }
    static final class GeneralContext extends BlockPlaceContext {
        GeneralContext(AttachmentVectors level,Actor actor,ItemStack stack){
            super(level,actor,net.minecraft.world.InteractionHand.MAIN_HAND,stack,new BlockHitResult(Vec3.atCenterOf(POS),Direction.UP,POS,false));
        }
    }
    static void run()throws Exception{
        var r=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),r.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new AttachmentVectors(r);var actor=new Actor(level);
        var fluids=List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8));
        var representatives=Set.of(Blocks.LANTERN,Blocks.LADDER,Blocks.WALL_TORCH,Blocks.AMETHYST_CLUSTER,Blocks.END_ROD,Blocks.GRINDSTONE);
        var targets=new ArrayList<Block>();
        for(var b:BuiltInRegistries.BLOCK)if(OWNERS.contains(b.getClass().getMethod("getStateForPlacement",BlockPlaceContext.class).getDeclaringClass())&&(b!=Blocks.LEVER&&!(b instanceof ButtonBlock)))targets.add(b);
        if(targets.size()!=21)throw new AssertionError(targets.size());
        for(var b:targets)if(!(b instanceof ScaffoldingBlock)){
            for(int mask=0;mask<64;mask++)for(var direction:Direction.values())for(var face:Direction.values())for(var fluid:fluids){
                level.mask(mask);level.raw(POS,fluid);
                var context=new DirectionalPlaceContext(level,POS,direction,ItemStack.EMPTY,face);
                emit("place\t"+BuiltInRegistries.BLOCK.getId(b)+"\t"+mask+"\t"+direction.get3DDataValue()+"\t"+face.get3DDataValue()+"\t"+id(fluid)+"\t"+id(b.getStateForPlacement(context)));
            }
            if(representatives.contains(b))for(int mask=0;mask<64;mask++)for(int yaw=0;yaw<360;yaw+=45)for(int pitch:new int[]{-80,-45,-10,10,45,80})for(var fluid:fluids){
                level.mask(mask);level.raw(POS,fluid);actor.setYRot(yaw);actor.setXRot(pitch);
                var context=new GeneralContext(level,actor,ItemStack.EMPTY);
                emit("player\t"+BuiltInRegistries.BLOCK.getId(b)+"\t"+mask+"\t"+yaw+"\t"+pitch+"\t"+id(fluid)+"\t"+id(b.getStateForPlacement(context)));
            }
            // Captured neighbor differs from the live world; survival queries
            // deliberately use the live support at the original state position.
            for(var state:b.getStateDefinition().getPossibleStates())for(var direction:Direction.values())for(int mask:new int[]{0,63})for(var captured:List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState())){
                if(b instanceof EndRodBlock)continue;
                level.mask(mask);level.raw(POS,state);level.trace.clear();
                var result=state.updateShape(level,level,POS,direction,POS.relative(direction),captured,level.random);
                emit("update\t"+id(state)+"\t"+direction.get3DDataValue()+"\t"+mask+"\t"+id(captured)+"\t"+id(result)+"\t"+String.join(",",level.trace.stream().filter(s->s.startsWith("fluid:")||s.startsWith("schedule:")).toList()));
            }
        }
        var atlas=new LinkedHashMap<Integer,BlockState>();
        for(var b:BuiltInRegistries.BLOCK)if(!b.hasDynamicShape())for(var state:b.getStateDefinition().getPossibleStates()){
            int bits=0;
            for(var side:Direction.values()){
                level.raw(POS.relative(side),state);int at=side.get3DDataValue()*2;
                if(state.isFaceSturdy(level,POS.relative(side),side.getOpposite()))bits|=1<<at;
                if(Block.canSupportCenter(level,POS.relative(side),side.getOpposite()))bits|=2<<at;
            }
            atlas.putIfAbsent(bits,state);
        }
        for(var b:targets)if(!(b instanceof EndRodBlock||b instanceof ScaffoldingBlock))for(var state:b.getStateDefinition().getPossibleStates())for(var support:atlas.values()){
            for(var side:Direction.values())level.raw(POS.relative(side),support);
            emit("survive\t"+id(state)+"\t"+id(support)+"\t"+state.canSurvive(level,POS));
        }
        // All end-rod support-facing pairs and ladder duplicate exclusions.
        for(var face:Direction.values())for(var support:Blocks.END_ROD.getStateDefinition().getPossibleStates()){
            level.mask(0);level.raw(POS,Blocks.AIR.defaultBlockState());level.raw(POS.relative(face.getOpposite()),support);
            var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,face);
            emit("rod\t"+face.get3DDataValue()+"\t"+id(support)+"\t"+id(Blocks.END_ROD.getStateForPlacement(c)));
        }
        for(var face:Direction.Plane.HORIZONTAL)for(var facing:Direction.Plane.HORIZONTAL)for(int replace=0;replace<2;replace++){
            level.mask(63);var old=Blocks.LADDER.defaultBlockState().setValue(LadderBlock.FACING,facing);
            level.raw(POS,replace==1?Blocks.AIR.defaultBlockState():Blocks.STONE.defaultBlockState());level.raw(POS.relative(face.getOpposite()),old);
            var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,face);
            emit("ladder\t"+face.get3DDataValue()+"\t"+id(old)+"\t"+replace+"\t"+id(Blocks.LADDER.getStateForPlacement(c)));
        }
        var scaffolds=new ArrayList<BlockState>();scaffolds.add(Blocks.AIR.defaultBlockState());
        for(int distance=0;distance<=7;distance++)scaffolds.add(Blocks.SCAFFOLDING.defaultBlockState().setValue(ScaffoldingBlock.DISTANCE,distance));
        var below=new ArrayList<>(scaffolds);below.add(Blocks.STONE.defaultBlockState());
        for(var base:below)for(int mask=0;mask<6561;mask++){
            level.raw(POS,Blocks.AIR.defaultBlockState());level.raw(POS.below(),base);
            int digits=mask;for(var side:SIDES){level.raw(POS.relative(side),scaffolds.get(digits%9));digits/=9;}
            var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
            emit("scaffold\t"+id(base)+"\t"+mask+"\t"+id(Blocks.SCAFFOLDING.getStateForPlacement(c))+"\t"+Blocks.SCAFFOLDING.defaultBlockState().canSurvive(level,POS));
        }
        for(var state:Blocks.SCAFFOLDING.getStateDefinition().getPossibleStates())for(var direction:Direction.values()){
            level.raw(POS,state);level.trace.clear();var at=POS.relative(direction);var old=level.getBlockState(at);
            var result=state.updateShape(level,level,POS,direction,at,old,level.random);
            emit("update\t"+id(state)+"\t"+direction.get3DDataValue()+"\t0\t"+id(old)+"\t"+id(result)+"\t"+String.join(",",level.trace.stream().filter(s->s.startsWith("fluid:")||s.startsWith("schedule:")).toList()));
        }
        for(var base:below)for(var fluid:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,1),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.IRON_CHAIN.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true))){
            level.mask(0);level.raw(POS.below(),base);level.raw(POS,fluid);
            var c=new DirectionalPlaceContext(level,POS,Direction.NORTH,ItemStack.EMPTY,Direction.UP);
            emit("scaffold-water\t"+id(base)+"\t"+id(fluid)+"\t"+id(Blocks.SCAFFOLDING.getStateForPlacement(c)));
        }
        for(var stack:List.of(ItemStack.EMPTY,new ItemStack(Blocks.SCAFFOLDING),new ItemStack(Blocks.SCAFFOLDING.asItem(),0),new ItemStack(Blocks.STONE))){
            level.raw(POS,Blocks.SCAFFOLDING.defaultBlockState());
            // Protected base constructor performs the real canBeReplaced call
            // without DirectionalPlaceContext's uninitialized direction query.
            var c=new GeneralContext(level,null,stack);
            emit("replace\t"+stack.getCount()+"\t"+BuiltInRegistries.ITEM.getId(stack.getItem())+"\t"+Blocks.SCAFFOLDING.defaultBlockState().canBeReplaced(c));
        }
        System.out.println("Native support atlas: "+atlas.size());
    }
}
final class AttachmentBootstrap {
    public static void main(String[] args)throws Exception{
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);AttachmentVectors.run();
    }
}
