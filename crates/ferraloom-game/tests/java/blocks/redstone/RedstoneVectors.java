import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.block.piston.PistonStructureResolver;
import net.minecraft.world.level.redstone.*;

/** Real vanilla SignalGetter, Level#setBlock, wire evaluators and piston resolver.
 * The fixture uses real chunks and CollectingNeighborUpdater, with unrelated
 * abstract services throwing. Scheduled-tick scenarios use the server harness. */
public final class RedstoneVectors extends MutationFixture {
    final net.minecraft.world.level.border.WorldBorder border = new net.minecraft.world.level.border.WorldBorder();
    final List<String> changes = new ArrayList<>();
    final List<String> notifications = new ArrayList<>();
    boolean recording;
    RedstoneVectors(RegistryAccess registries) { super(registries); }
    @Override public net.minecraft.world.level.border.WorldBorder getWorldBorder() { return border; }
    @Override public void sendBlockUpdated(BlockPos pos, BlockState old, BlockState state, int flags) {
        if (recording) changes.add(pos.toShortString()+"="+Block.getId(state)+"/"+flags);
    }
    @Override public void updateNeighborsAt(BlockPos pos, Block block) {
        updateNeighborsAt(pos,block,ExperimentalRedstoneUtils.initialOrientation(this,null,null));
    }
    @Override public void updateNeighborsAt(BlockPos pos, Block block, Orientation orientation) {
        if (recording) notifications.add(pos.toShortString()+"/"+(orientation==null?-1:orientation.getIndex()));
        neighborUpdater.updateNeighborsAtExceptFromFacing(pos,block,null,orientation);
    }
    @Override public void updateNeighborsAtExceptFromFacing(BlockPos pos, Block block, Direction skip, Orientation orientation) {
        neighborUpdater.updateNeighborsAtExceptFromFacing(pos,block,skip,orientation);
    }
    @Override public void neighborChanged(BlockPos pos, Block block, Orientation orientation) { neighborUpdater.neighborChanged(pos,block,orientation); }
    @Override public void neighborChanged(BlockState state,BlockPos pos,Block block,Orientation orientation,boolean moved) { neighborUpdater.neighborChanged(state,pos,block,orientation,moved); }
    void raw(BlockPos pos,BlockState state) { getChunkAt(pos).setBlockState(pos,state,816); }
    void begin() { recording=true; changes.clear(); notifications.clear(); trace.clear(); }
    static String positions(List<BlockPos> list) { return String.join(";",list.stream().map(BlockPos::toShortString).toList()); }
    static final BlockPos ORIGIN = new BlockPos(8,0,8);
    static void signals(RegistryAccess r) {
        for (var block:List.of(Blocks.REDSTONE_BLOCK,Blocks.REDSTONE_TORCH,Blocks.REDSTONE_WALL_TORCH,Blocks.OBSERVER,Blocks.REPEATER,Blocks.LEVER,Blocks.STONE_BUTTON,Blocks.LIGHT_WEIGHTED_PRESSURE_PLATE)) {
            for (var state:block.getStateDefinition().getPossibleStates()) {
                // Every direction, all powered/orientation/delay/button states.
                var level=new RedstoneVectors(r);level.raw(ORIGIN,state);
                var values=new ArrayList<String>();
                values.add(""+state.isSignalSource());values.add(""+state.getOwnSignal(level,ORIGIN));
                for(var d:Direction.values()) {
                    values.add(level.getSignal(ORIGIN,d)+":"+level.getDirectSignal(ORIGIN,d)+":"+level.getControlInputSignal(ORIGIN,d,false)+":"+level.getControlInputSignal(ORIGIN,d,true));
                }
                System.out.println("signal:"+Block.getId(state)+"\t"+String.join(";",values));
            }
        }
        for(var source:List.of(Blocks.REDSTONE_BLOCK.defaultBlockState(),Blocks.REDSTONE_TORCH.defaultBlockState(),Blocks.OBSERVER.defaultBlockState().setValue(ObserverBlock.POWERED,true)))for(var d:Direction.values()) {
            var level=new RedstoneVectors(r);level.raw(ORIGIN,Blocks.STONE.defaultBlockState());level.raw(ORIGIN.relative(d),source);
            System.out.println("relay:"+Block.getId(source)+":"+d.get3DDataValue()+"\t"+level.getSignal(ORIGIN,Direction.DOWN)+":"+level.getDirectSignalTo(ORIGIN)+":"+level.getBestNeighborSignal(ORIGIN)+":"+level.hasNeighborSignal(ORIGIN));
        }
    }
    static void piston(RegistryAccess r) {
        for(boolean extending:new boolean[]{true,false})for(var direction:Direction.values())for(int length:new int[]{0,1,11,12,13}) {
            var level=new RedstoneVectors(r);var piston=ORIGIN;
            for(int i=0;i<length;i++)level.raw(piston.relative(direction,i+(extending?1:2)),Blocks.STONE.defaultBlockState());
            var resolver=new PistonStructureResolver(level,piston,direction,extending);boolean ok=resolver.resolve();
            System.out.println("piston-line:"+direction.get3DDataValue()+":"+extending+":"+length+"\t"+ok+"|"+positions(resolver.getToPush())+"|"+positions(resolver.getToDestroy()));
        }
        for(var sticky:List.of(Blocks.SLIME_BLOCK,Blocks.HONEY_BLOCK))for(int kind=0;kind<6;kind++) {
            var level=new RedstoneVectors(r);var s=ORIGIN.east();level.raw(s,sticky.defaultBlockState());
            level.raw(s.above(),(kind==1?Blocks.OBSIDIAN:kind==2?Blocks.REDSTONE_WIRE:kind==3?(sticky==Blocks.SLIME_BLOCK?Blocks.HONEY_BLOCK:Blocks.SLIME_BLOCK):sticky).defaultBlockState());
            level.raw(s.below(),Blocks.STONE.defaultBlockState());level.raw(s.north(),Blocks.STONE.defaultBlockState());level.raw(s.south(),Blocks.STONE.defaultBlockState());
            if(kind==4){level.raw(s.east(),sticky.defaultBlockState());level.raw(s.above().east(),sticky.defaultBlockState());level.raw(s.above().east(2),sticky.defaultBlockState());}
            if(kind==5)for(int i=0;i<12;i++)level.raw(s.south(i+1),Blocks.STONE.defaultBlockState());
            var resolver=new PistonStructureResolver(level,ORIGIN,Direction.EAST,true);boolean ok=resolver.resolve();
            System.out.println("piston-branch:"+(sticky==Blocks.SLIME_BLOCK?"slime":"honey")+":"+kind+"\t"+ok+"|"+positions(resolver.getToPush())+"|"+positions(resolver.getToDestroy()));
        }
    }
    static void wires(RegistryAccess r) {
        for(boolean experimental:new boolean[]{false,true})for(int shape=0;shape<3;shape++) {
            var level=new RedstoneVectors(r);level.experiments=experimental;level.random.setSeed(0);
            var wires=new ArrayList<BlockPos>();
            for(int x=0;x<7;x++)wires.add(ORIGIN.east(x));
            if(shape>0)for(int z=1;z<=3;z++){wires.add(ORIGIN.east(2).south(z));wires.add(ORIGIN.east(4).north(z));}
            if(shape==2){wires.add(ORIGIN.east(3).above());level.raw(ORIGIN.east(3),Blocks.STONE.defaultBlockState());wires.remove(ORIGIN.east(3));}
            for(var pos:wires){level.raw(pos.below(),Blocks.STONE.defaultBlockState());level.raw(pos,Blocks.REDSTONE_WIRE.defaultBlockState());}
            for(var pos:wires)level.raw(pos,Block.updateFromNeighbourShapes(level.getBlockState(pos),level,pos));
            for(int change=0;change<3;change++) {
                var power=ORIGIN.west();level.begin();
                level.setBlock(power,(change==1?Blocks.AIR:Blocks.REDSTONE_BLOCK).defaultBlockState(),3);
                var states=new ArrayList<String>();for(var pos:wires)states.add(pos.toShortString()+"="+Block.getId(level.getBlockState(pos)));
                System.out.println("wire:"+experimental+":"+shape+":"+change+"\t"+String.join(";",states)+"|"+String.join(";",level.changes)+"|"+String.join(";",level.notifications));
            }
        }
    }
    public static void main(String[] args) {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
        var registries=NeighborVectors.load();
        signals(registries);piston(registries);wires(registries);
    }
}

// Bootstrap before loading any Level subclass: Level's own static fields refer
// to particle/game-event registries, which must already be initialized.
final class RedstoneBootstrap {
    public static void main(String[] args) {
        var out=System.out;var err=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
        RedstoneVectors.main(args);
    }
}
