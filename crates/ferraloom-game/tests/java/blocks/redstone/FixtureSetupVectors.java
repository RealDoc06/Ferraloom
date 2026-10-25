import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.arguments.blocks.BlockInput;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.Level;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.util.ProblemReporter;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.phys.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.piston.PistonStructureResolver;
import net.minecraft.world.level.block.piston.PistonBaseBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Audit the new live fixtures with actual BlockInput, real ServerLevel
 * callbacks/ticks and the native piston resolver before the builder round. */
public final class FixtureSetupVectors {
    static final BlockPos P = new BlockPos(8,64,8);
    static void check(boolean condition, String message) { if (!condition) throw new AssertionError(message); }
    static void place(DeviceVectors world, BlockPos pos, BlockState state) {
        var old = world.getBlockState(pos);
        if (new BlockInput(state, Set.copyOf(state.getProperties()), null).place(world,pos,258)) {
            world.updateNeighboursOnBlockSet(pos,old);
        }
    }
    static void tick(DeviceVectors world, int time) {
        world.time=time;
        world.ticks.tick(time,65536,(pos,block)->{var state=world.getBlockState(pos);if(state.is(block))state.tick(world,pos,world.getRandom());});
    }
    static void rods(DeviceVectors world) {
        for (String name : List.of("lightning_rod","exposed_lightning_rod","weathered_lightning_rod","oxidized_lightning_rod","waxed_lightning_rod","waxed_exposed_lightning_rod","waxed_weathered_lightning_rod","waxed_oxidized_lightning_rod")) {
            var block=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(name));
            for (Direction facing : Direction.values()) {
                world.prepare(Blocks.AIR.defaultBlockState());
                for (Direction output : Direction.values()) {
                    place(world,P.relative(output),Blocks.STONE.defaultBlockState());
                    place(world,P.relative(output,2),Blocks.REDSTONE_LAMP.defaultBlockState());
                }
                place(world,P,block.defaultBlockState().setValue(LightningRodBlock.FACING,facing).setValue(LightningRodBlock.POWERED,true));
                for (Direction output : Direction.values()) place(world,P.relative(output,3),Blocks.STONE.defaultBlockState());
                for (int time=1;time<=12;time++) {
                    tick(world,time);
                    check(world.getBlockState(P).getValue(LightningRodBlock.POWERED)==(time<8),name+" power "+time);
                    for (Direction output : Direction.values()) check(world.getBlockState(P.relative(output,2)).getValue(RedstoneLampBlock.LIT)==(time<12&&output==facing.getOpposite()),name+" "+facing+" "+output+" at "+time);
                }
            }
        }
    }
    static void rails(DeviceVectors world) {
        world.prepare(Blocks.AIR.defaultBlockState());
        for(int i=0;i<11;i++) place(world,P.east(i).above(i>=4?1:0).below(),Blocks.STONE.defaultBlockState());
        for(int i=0;i<11;i++) {
            var pos=P.east(i).above(i>=4?1:0);
            place(world,pos,Blocks.POWERED_RAIL.defaultBlockState().setValue(PoweredRailBlock.SHAPE,i==3?RailShape.ASCENDING_EAST:RailShape.EAST_WEST));
            check(world.getBlockState(pos).is(Blocks.POWERED_RAIL),"rail support at "+i);
        }
        check(world.getBlockState(P.east(3)).getValue(PoweredRailBlock.SHAPE)==RailShape.ASCENDING_EAST,"ascending rail shape");
        place(world,P.below(),Blocks.REDSTONE_BLOCK.defaultBlockState());
        for(int i=0;i<11;i++) check(world.getBlockState(P.east(i).above(i>=4?1:0)).getValue(PoweredRailBlock.POWERED)==(i<9),"rail propagation at "+i);
    }
    static void collision(DeviceVectors world) throws Exception {
        world.prepare(Blocks.PISTON.defaultBlockState().setValue(PistonBaseBlock.FACING,Direction.EAST));
        var root=P.east();
        for(var pos:List.of(root,root.above(),root.above().west(),root.above().west().north()))place(world,pos,Blocks.SLIME_BLOCK.defaultBlockState());
        place(world,root.north(),Blocks.REDSTONE_BLOCK.defaultBlockState());
        place(world,root.west().north(),Blocks.STONE.defaultBlockState());
        var junction=root.north(2);
        for(var pos:List.of(junction,junction.north(),junction.west(),junction.east()))place(world,pos.below(),Blocks.STONE.defaultBlockState());
        for(var pos:List.of(junction,junction.north(),junction.west(),junction.east()))place(world,pos,Blocks.RAIL.defaultBlockState());
        var before=world.getBlockState(junction).getValue(RailBlock.SHAPE);
        var popped=List.of(root.east(),root.north().east(),root.above().east());
        for(var pos:popped.subList(0,2))place(world,pos.below(),Blocks.OBSIDIAN.defaultBlockState());
        place(world,popped.get(0),Blocks.REDSTONE_WIRE.defaultBlockState());
        place(world,popped.get(1),Blocks.REDSTONE_TORCH.defaultBlockState());
        place(world,popped.get(2),Blocks.COBWEB.defaultBlockState());
        var resolver=new PistonStructureResolver(world,P,Direction.EAST,true);
        check(resolver.resolve(),"collision push rejected");
        check(resolver.getToPush().indexOf(root.west().north())<resolver.getToPush().indexOf(root.north()),"collision did not reorder");
        var move=PistonBaseBlock.class.getDeclaredMethod("moveBlocks",Level.class,BlockPos.class,Direction.class,boolean.class);move.setAccessible(true);
        check((boolean)move.invoke(Blocks.PISTON,world,P,Direction.EAST,true),"collision movement failed");
        var after=world.getBlockState(junction).getValue(RailBlock.SHAPE);
        check(before==RailShape.NORTH_WEST&&after==RailShape.NORTH_EAST,"junction order witness: "+before+" -> "+after);
    }
    static void electrical(DeviceVectors world) {
        world.prepare(Blocks.AIR.defaultBlockState());
        for(var pos:List.of(P.below(),P.east(),P.above()))place(world,pos,Blocks.STONE.defaultBlockState());
        for(var pos:List.of(P.west(),P.east(2),P.above(2)))place(world,pos,Blocks.REDSTONE_LAMP.defaultBlockState());
        place(world,P,Blocks.REDSTONE_TORCH.defaultBlockState());tick(world,1);
        check(world.getBlockState(P.west()).getValue(RedstoneLampBlock.LIT),"torch weak side output");
        check(!world.getBlockState(P.east(2)).getValue(RedstoneLampBlock.LIT),"weak side output relayed");
        check(world.getBlockState(P.above(2)).getValue(RedstoneLampBlock.LIT),"torch direct top output");
        for(int output:new int[]{-7,0,5,23}) {
            world.prepare(Blocks.AIR.defaultBlockState());
            place(world,P,Blocks.COMPARATOR.defaultBlockState().setValue(ComparatorBlock.FACING,Direction.WEST));
            for(int time=1;time<=3;time++)tick(world,time);
            var entity=(ComparatorBlockEntity)world.getBlockEntity(P);
            var data=entity.saveWithFullMetadata(world.registryAccess());data.putInt("OutputSignal",output);
            entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,world.registryAccess(),data));entity.setChanged();
            place(world,P,world.getBlockState(P).setValue(ComparatorBlock.POWERED,true));
            place(world,P.east(),Blocks.STONE.defaultBlockState());place(world,P.east(2),Blocks.REDSTONE_LAMP.defaultBlockState());place(world,P.east(3),Blocks.STONE.defaultBlockState());
            tick(world,4);
            check(entity.getOutputSignal()==output,"stored comparator output changed before tick 2");
            check(world.getBlockState(P.east(2)).getValue(RedstoneLampBlock.LIT)==(output>0),"signed comparator relay "+output);
        }
        world.prepare(Blocks.AIR.defaultBlockState());
        place(world,P.below(),Blocks.REDSTONE_BLOCK.defaultBlockState());place(world,P,Blocks.REDSTONE_WIRE.defaultBlockState());
        for(var direction:List.of(Direction.NORTH,Direction.SOUTH,Direction.EAST,Direction.WEST,Direction.UP)) {
            place(world,P.relative(direction),Blocks.REDSTONE_LAMP.defaultBlockState());place(world,P.relative(direction,2),Blocks.STONE.defaultBlockState());
        }
        tick(world,1);
        var actor=new DeviceVectors.Actor(world);
        for(int toggle=0;toggle<2;toggle++) {
            var before=world.getBlockState(P);
            before.useWithoutItem(world,actor,new BlockHitResult(Vec3.atCenterOf(P),Direction.UP,P,false));
            check(world.getBlockState(P)!=before,"dot/cross toggle");
            for(int time=2+toggle*5;time<=6+toggle*5;time++)tick(world,time);
            for(var direction:List.of(Direction.NORTH,Direction.SOUTH,Direction.EAST,Direction.WEST)) check(world.getBlockState(P.relative(direction)).getValue(RedstoneLampBlock.LIT)==(toggle==0),"dot/cross side output");
            check(!world.getBlockState(P.above()).getValue(RedstoneLampBlock.LIT),"wire downward query output");
        }
    }
    static void hoppers(DeviceVectors world) {
        world.prepare(Blocks.AIR.defaultBlockState());
        var hoppers=new ArrayList<HopperBlockEntity>();
        for(int i=0;i<4;i++) {
            var pos=P.east(i);
            place(world,pos,Blocks.HOPPER.defaultBlockState().setValue(HopperBlock.FACING,Direction.EAST));
            var entity=(HopperBlockEntity)world.getBlockEntity(pos);
            var tag=new CompoundTag();tag.putInt("TransferCooldown",0);
            entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,world.registryAccess(),tag));
            hoppers.add(entity);
        }
        place(world,P.east(4),Blocks.BARREL.defaultBlockState());
        place(world,P.above(),Blocks.BARREL.defaultBlockState());
        var source=(BarrelBlockEntity)world.getBlockEntity(P.above());
        source.setItem(0,new ItemStack(Items.DIRT,3));source.setItem(1,new ItemStack(Items.STONE,2));
        for(int time=0;time<=96;time++) {
            world.time=time;
            if(time==0||time==24)place(world,P.north(),Blocks.REDSTONE_BLOCK.defaultBlockState());
            if(time==10||time==32)place(world,P.north(),Blocks.AIR.defaultBlockState());
            if(time>0)for(int i=0;i<4;i++)HopperBlockEntity.pushItemsTick(world,P.east(i),world.getBlockState(P.east(i)),hoppers.get(i));
        }
        var sink=(BarrelBlockEntity)world.getBlockEntity(P.east(4));
        check(source.isEmpty(),"hopper source did not empty");
        check(sink.getItem(0).is(Items.DIRT)&&sink.getItem(0).getCount()==3&&sink.getItem(1).is(Items.STONE)&&sink.getItem(1).getCount()==2,"hopper chain did not drain at 96");
    }
    static void dispensers(DeviceVectors world) {
        for(var block:List.of(Blocks.DISPENSER,Blocks.DROPPER)) {
            world.prepare(block.defaultBlockState().setValue(DispenserBlock.FACING,Direction.EAST));
            var entity=(DispenserBlockEntity)world.getBlockEntity(P);entity.setItem(0,new ItemStack(Items.STONE,6));
            if(block==Blocks.DROPPER)place(world,P.east(),Blocks.BARREL.defaultBlockState());
            place(world,P.above(2),Blocks.REDSTONE_BLOCK.defaultBlockState());
            for(int time=1;time<=3;time++)tick(world,time);
            check(!world.getBlockState(P).getValue(DispenserBlock.TRIGGERED),"quasi power invented a wake");
            place(world,P.west(),Blocks.STONE.defaultBlockState());
            for(int time=4;time<=11;time++) {
                tick(world,time);check(entity.getItem(0).getCount()==(time<7?6:5),"dispense tick: "+time);
            }
            place(world,P.above(2),Blocks.AIR.defaultBlockState());place(world,P.west(),Blocks.AIR.defaultBlockState());
            check(!world.getBlockState(P).getValue(DispenserBlock.TRIGGERED),"quasi release did not wake");
            for(int pulse=0;pulse<3;pulse++) {place(world,P.north(),Blocks.REDSTONE_BLOCK.defaultBlockState());place(world,P.north(),Blocks.AIR.defaultBlockState());}
            for(int time=12;time<=19;time++)tick(world,time);
            check(entity.getItem(0).getCount()==4,"dispense dedup");
            if(block==Blocks.DROPPER)check(((BarrelBlockEntity)world.getBlockEntity(P.east())).getItem(0).getCount()==2,"dropper destination");
        }
    }
    static void openings(DeviceVectors world) {
        for(String name:List.of("oak_door","iron_door","copper_door","oxidized_copper_door","waxed_copper_door","oak_trapdoor","iron_trapdoor","copper_trapdoor","oak_fence_gate","bamboo_fence_gate")) {
            var block=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(name));
            world.prepare(Blocks.AIR.defaultBlockState());
            var actor=new DeviceVectors.Actor(world);actor.setPos(Vec3.atCenterOf(P.south(2)));actor.getAbilities().instabuild=true;
            var item=new ItemStack(block.asItem());actor.getInventory().setItem(0,item);
            var floorHit=new BlockHitResult(Vec3.atCenterOf(P.below()).add(0,.5,0),Direction.UP,P.below(),false);
            item.getItem().useOn(new UseOnContext(actor,InteractionHand.MAIN_HAND,floorHit));
            check(world.getBlockState(P).is(block),"item placement: "+block);
            actor.getInventory().setItem(0,ItemStack.EMPTY);
            for(int toggle=0;toggle<2;toggle++) {
                world.getBlockState(P).useWithoutItem(world,actor,new BlockHitResult(Vec3.atCenterOf(P),Direction.SOUTH,P,false));
                boolean manual=block!=Blocks.IRON_DOOR&&block!=Blocks.IRON_TRAPDOOR;
                check(world.getBlockState(P).getValue(BlockStateProperties.OPEN)==(manual&&toggle==0),"manual opening: "+block);
            }
            for(boolean powered:new boolean[]{true,false}) {
                place(world,P.west(),(powered?Blocks.REDSTONE_BLOCK:Blocks.AIR).defaultBlockState());
                check(world.getBlockState(P).getValue(BlockStateProperties.OPEN)==powered&&world.getBlockState(P).getValue(BlockStateProperties.POWERED)==powered,"redstone opening: "+block);
            }
        }
    }
    static void hooks(DeviceVectors world) {
        for(int distance:new int[]{2,10,41,42}) {
            world.prepare(Blocks.AIR.defaultBlockState());
            for(int i=0;i<=distance+1;i++)place(world,P.east(i).below(),Blocks.STONE.defaultBlockState());
            place(world,P.west(),Blocks.STONE.defaultBlockState());place(world,P.east(distance+1),Blocks.STONE.defaultBlockState());
            place(world,P,Blocks.TRIPWIRE_HOOK.defaultBlockState().setValue(TripWireHookBlock.FACING,Direction.EAST));
            place(world,P.east(distance),Blocks.TRIPWIRE_HOOK.defaultBlockState().setValue(TripWireHookBlock.FACING,Direction.WEST));
            for(int i=1;i<distance;i++)place(world,P.east(i),Blocks.TRIPWIRE.defaultBlockState());
            check(world.getBlockState(P).getValue(TripWireHookBlock.ATTACHED)==(distance<42),"hook length: "+distance);
        }
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"fixture-audit")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var world=new DeviceVectors(server,storage);
        rods(world);rails(world);collision(world);electrical(world);hoppers(world);dispensers(world);openings(world);hooks(world);
        System.out.println("Native fixture audit passed: 48 rod timelines, ascending rail supports/limit, collision reorder junction, weak/direct and signed signals, dot/cross, hopper locks/drain, quasi dispense/dedup, 10 opening types, hook boundaries");
        System.exit(0);
    }
}
