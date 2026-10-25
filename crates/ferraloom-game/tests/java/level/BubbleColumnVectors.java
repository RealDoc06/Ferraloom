import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.ticks.*;

/** Calls the actual 26.3 BubbleColumnBlock virtuals and column algorithm on
 * the existing real ServerLevel fixture. No bubble behavior is mocked. */
final class BubbleColumnVectors {
    static final BlockPos POS=DestroyBlockVectors.POS;
    static BlockState bubble(boolean drag){return Blocks.BUBBLE_COLUMN.defaultBlockState().setValue(BlockStateProperties.DRAG,drag);}
    static int id(BlockState state){return Block.getId(state);}
    static void raw(DestroyBlockVectors level,BlockPos pos,BlockState state){
        var chunk=level.getChunkAt(pos);
        chunk.getSections()[chunk.getSectionIndex(pos.getY())].setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,state);
    }
    static String states(BlockState[] states){return Arrays.stream(states).map(s->""+id(s)).collect(java.util.stream.Collectors.joining(","));}
    static class Ticks implements ScheduledTickAccess {
        final List<String> calls=new ArrayList<>();
        @Override public <T> ScheduledTick<T> createTick(BlockPos p,T t,int d,TickPriority priority){throw new AssertionError("unused createTick");}
        @Override public <T> ScheduledTick<T> createTick(BlockPos p,T t,int d){throw new AssertionError("unused createTick");}
        @Override public LevelTickAccess<Block> getBlockTicks(){throw new AssertionError("unused block queue");}
        @Override public LevelTickAccess<Fluid> getFluidTicks(){throw new AssertionError("unused fluid queue");}
        @Override public void scheduleTick(BlockPos p,Block b,int d){calls.add("block:"+d);}
        @Override public void scheduleTick(BlockPos p,Fluid f,int d){calls.add("fluid:"+d);}
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory("bubble-column")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new net.minecraft.server.players.PlayerList(server,server.registries(),storage.createPlayerStorage(),new net.minecraft.server.notifications.EmptyNotificationService()) {});
        var level=new DestroyBlockVectors(server,storage);
        BlockState[] below={Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.SOUL_SAND.defaultBlockState(),Blocks.MAGMA_BLOCK.defaultBlockState(),bubble(false),bubble(true)};
        BlockState[] neighbors={Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,1),bubble(false),bubble(true),Blocks.OAK_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true)};
        for(boolean drag:new boolean[]{false,true})for(var support:below){
            level.chunks.clear();raw(level,POS.below(),support);
            System.out.println("BUBBLE\tsurvive\t"+drag+"\t"+id(support)+"\t"+bubble(drag).canSurvive(level,POS));
            for(Direction direction:Direction.values())for(var neighbor:neighbors){
                var ticks=new Ticks();
                var result=bubble(drag).updateShape(level,ticks,POS,direction,POS.relative(direction),neighbor,level.getRandom());
                System.out.println("BUBBLE\tshape\t"+drag+"\t"+id(support)+"\t"+direction.ordinal()+"\t"+id(neighbor)+"\t"+id(result)+"\t"+String.join(";",ticks.calls));
            }
        }
        BlockState water=Blocks.WATER.defaultBlockState(),air=Blocks.AIR.defaultBlockState(),stone=Blocks.STONE.defaultBlockState();
        BlockState[][] towers={
            {water,water,water,air},{bubble(false),water,water,air},
            {bubble(true),bubble(false),bubble(true),air},{bubble(false),bubble(false),water,air},
            {water,Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,1),water,air},
            {water,Blocks.OAK_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true),water,air},
            {air,water,water,air},{bubble(true),stone,water,air}
        };
        for(var support:below)for(var tower:towers)for(boolean tick:new boolean[]{false,true}){
            if(tick&&!tower[0].is(Blocks.BUBBLE_COLUMN)&&!tower[0].is(Blocks.WATER))continue;
            level.chunks.clear();raw(level,POS.below(),support);
            for(int y=0;y<tower.length;y++)raw(level,POS.above(y),tower[y]);
            level.getRandom().setSeed(42);
            if(tick)tower[0].tick(level,POS,level.getRandom());
            else BubbleColumnBlock.updateColumn(Blocks.BUBBLE_COLUMN,level,POS,tower[0],support);
            BlockState[] result=new BlockState[4];for(int y=0;y<4;y++)result[y]=level.getBlockState(POS.above(y));
            System.out.println("BUBBLE\tcolumn\t"+tick+"\t"+id(support)+"\t"+states(tower)+"\t"+states(result)+"\t"+level.getRandom().nextLong());
        }
        BlockState[] above={air,stone,water,bubble(false),Blocks.COBWEB.defaultBlockState(),Blocks.MOVING_PISTON.defaultBlockState()};
        for(var state:above)for(boolean drag:new boolean[]{false,true})for(boolean precise:new boolean[]{false,true})for(double speed:new double[]{-1.2,0,0.69,1.79}){
            level.chunks.clear();raw(level,POS.above(),state);
            var entity=new ItemEntity(level,8.5,64,8.5,new ItemStack(Items.STONE));entity.setDeltaMovement(new Vec3(.25,speed,-.125));entity.fallDistance=8;
            level.getRandom().setSeed(42);
            bubble(drag).entityInside(level,POS,entity,null,precise);
            System.out.println("BUBBLE\tinside\t"+id(state)+"\t"+drag+"\t"+precise+"\t"+DestroyBlockVectors.bits(speed)+"\t"+DestroyBlockVectors.bits(entity.getDeltaMovement().y)+"\t"+DestroyBlockVectors.bits(entity.fallDistance)+"\t"+level.getRandom().nextLong());
        }
        for(boolean drag:new boolean[]{false,true})for(boolean present:new boolean[]{false,true}){
            level.chunks.clear();raw(level,POS,present?bubble(drag):air);level.trace.clear();level.recording=true;
            var stack=((BubbleColumnBlock)Blocks.BUBBLE_COLUMN).pickupBlock(null,level,POS,bubble(drag));level.recording=false;
            System.out.println("BUBBLE\tpickup\t"+drag+"\t"+present+"\t"+BuiltInRegistries.ITEM.getKey(stack.getItem())+"\t"+stack.getCount()+"\t"+id(level.getBlockState(POS))+"\t"+String.join(";",level.trace));
        }
        System.out.flush();System.exit(0);
    }
}
class BubbleColumnCapture {
    public static void main(String[] args)throws Exception{
        var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);BubbleColumnVectors.run();
    }
}
