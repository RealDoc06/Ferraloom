import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.*;
import net.minecraft.world.level.gameevent.vibrations.*;
import net.minecraft.world.phys.Vec3;

/** Real sculk users, tickers, listeners and lifecycle on a disposable ServerLevel. */
public final class VibrationOracle {
    static Path output;
    static final AtomicBoolean ran=new AtomicBoolean();
    static final BlockPos POS=new BlockPos(8,80,8);
    public static Boolean tickingOverride=null;
    static final List<String> changes=new ArrayList<>();
    public static void premain(String argument,Instrumentation instrumentation) {
        output=Path.of(argument);
        new AgentBuilder.Default().disableClassFormatChanges().with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .type(named("net.minecraft.server.level.ServerLevel"))
            .transform((builder,type,loader,module,domain)->builder.visit(Advice.to(OnTick.class).on(named("tick"))).visit(Advice.to(OnTickEligibility.class).on(named("shouldTickBlocksAt").and(takesArguments(long.class)))))
            .installOn(instrumentation);
        new AgentBuilder.Default().disableClassFormatChanges().with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .type(named("net.minecraft.world.level.block.entity.BlockEntity"))
            .transform((builder,type,loader,module,domain)->builder.visit(Advice.to(OnChanged.class).on(named("setChanged").and(takesArguments(0)))))
            .installOn(instrumentation);
    }
    public static class OnTick { @Advice.OnMethodEnter public static void before(@Advice.This Object level) { VibrationOracle.run(level); } }
    public static class OnTickEligibility { @Advice.OnMethodExit public static void after(@Advice.Return(readOnly=false) boolean value) { if(VibrationOracle.tickingOverride!=null)value=VibrationOracle.tickingOverride; } }
    public static class OnChanged { @Advice.OnMethodEnter public static void before(@Advice.This Object entity) { VibrationOracle.changed(entity); } }
    public static void changed(Object value) {
        if(value instanceof BlockEntity entity && entity.getBlockPos().equals(POS) && value instanceof VibrationSystem system) {
            var data=system.getVibrationData();
            changes.add(data.getTravelTimeInTicks()+","+(data.getCurrentVibration()!=null)+","+data.getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
        }
    }
    static VibrationSystem system(ServerLevel level,BlockState state) {
        level.setBlock(POS,Blocks.AIR.defaultBlockState(),816);
        level.removeBlockEntity(POS);
        level.setBlock(POS,state,3);
        level.getBlockTicks().clearArea(new net.minecraft.world.level.levelgen.structure.BoundingBox(0,0,0,15,255,15));
        changes.clear();
        return (VibrationSystem)level.getBlockEntity(POS);
    }
    static String summary(ServerLevel level,VibrationSystem system) {
        var data=system.getVibrationData();
        int frequency=system instanceof SculkSensorBlockEntity sensor?sensor.getLastVibrationFrequency():0;
        return Block.getId(level.getBlockState(POS))+"\t"+frequency+"\t"+(data.getCurrentVibration()!=null)+"\t"+data.getTravelTimeInTicks()+"\t"+data.getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent()+"\t"+data.shouldReloadVibrationParticle()+"\t"+String.join(";",changes)+"\t"+level.getRandom().nextLong();
    }
    public static void run(Object value) {
        if(!ran.compareAndSet(false,true)) return;
        var level=(ServerLevel)value;
        try {
            var lines=new ArrayList<String>();
            for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++)level.getChunk(x,z);
            for(var entry:BuiltInRegistries.GAME_EVENT.listElements().toList())
                lines.add("frequency\t"+entry.key().identifier()+"\t"+VibrationSystem.getGameEventFrequency(entry));
            for(var first:List.of(GameEvent.STEP,GameEvent.BLOCK_CHANGE,GameEvent.BLOCK_PLACE))
                for(var second:List.of(GameEvent.STEP,GameEvent.BLOCK_CHANGE,GameEvent.BLOCK_PLACE))
                    for(float distance: new float[]{0,1,2,Float.NaN})for(long offset:new long[]{-1,0,1}) {
                        var selector=new VibrationSelector();
                        selector.addCandidate(new VibrationInfo(first,1,Vec3.ZERO,(UUID)null,null),10);
                        selector.addCandidate(new VibrationInfo(second,distance,Vec3.ZERO,(UUID)null,null),10+offset);
                        var selected=selector.chosenCandidate(12).orElseThrow();
                        lines.add("selector\t"+first.unwrapKey().orElseThrow().identifier()+"\t"+second.unwrapKey().orElseThrow().identifier()+"\t"+Float.floatToRawIntBits(distance)+"\t"+offset+"\t"+selected.gameEvent().unwrapKey().orElseThrow().identifier()+"\t"+Float.floatToRawIntBits(selected.distance())+"\t"+selector.chosenCandidate(10).isPresent());
                    }
            for(var block:List.of(Blocks.SCULK_SENSOR,Blocks.CALIBRATED_SCULK_SENSOR,Blocks.SCULK_SHRIEKER)) {
                for(float distance:new float[]{0,1,2.9f,8,16})for(boolean reload:List.of(false,true))for(int delay:new int[]{0,1,3})for(boolean eligible:List.of(false,true)) {
                    tickingOverride=eligible;
                    var system=system(level,block.defaultBlockState());
                    var data=system.getVibrationData();
                    data.setCurrentVibration(new VibrationInfo(GameEvent.BLOCK_PLACE,distance,Vec3.atCenterOf(POS.east(3)),(UUID)null,null));
                    data.setTravelTimeInTicks(delay);data.setReloadVibrationParticle(reload);
                    level.getRandom().setSeed(1234);changes.clear();
                    VibrationSystem.Ticker.tick(level,data,system.getVibrationUser());
                    lines.add("tick\t"+Block.getId(block.defaultBlockState())+"\t"+Float.floatToRawIntBits(distance)+"\t"+reload+"\t"+delay+"\t"+eligible+"\t"+summary(level,system));
                }
                for(int offset:new int[]{-1,0,1})for(float distance:new float[]{0,1,3})for(boolean eligible:List.of(false,true)) {
                    tickingOverride=eligible;
                    var system=system(level,block.defaultBlockState());
                    var data=system.getVibrationData();
                    data.getSelectionStrategy().addCandidate(new VibrationInfo(GameEvent.BLOCK_PLACE,distance,Vec3.atCenterOf(POS.east(3)),(UUID)null,null),level.getGameTime()+offset);
                    level.getRandom().setSeed(1234);changes.clear();VibrationSystem.Ticker.tick(level,data,system.getVibrationUser());
                    lines.add("select-tick\t"+Block.getId(block.defaultBlockState())+"\t"+Float.floatToRawIntBits(distance)+"\t"+offset+"\t"+eligible+"\t"+summary(level,system));
                }
            }
            for(var block:List.of(Blocks.SCULK_SENSOR,Blocks.CALIBRATED_SCULK_SENSOR))for(var phase:net.minecraft.world.level.block.state.properties.SculkSensorPhase.values())for(boolean wet:List.of(false,true))for(float distance:new float[]{0,1,7.99f,8,16}) {
                var state=block.defaultBlockState().setValue(SculkSensorBlock.PHASE,phase).setValue(SculkSensorBlock.WATERLOGGED,wet);
                var system=system(level,state);
                level.getRandom().setSeed(1234);changes.clear();
                system.getVibrationUser().onReceiveVibration(level,POS.east(3),GameEvent.BLOCK_PLACE,null,null,distance);
                lines.add("receive\t"+Block.getId(state)+"\t"+Float.floatToRawIntBits(distance)+"\t"+summary(level,system));
            }
            for(var block:List.of(Blocks.SCULK_SENSOR,Blocks.CALIBRATED_SCULK_SENSOR,Blocks.SCULK_SHRIEKER))for(boolean same:List.of(false,true))for(boolean wool:List.of(false,true))for(boolean blocked:List.of(false,true))for(boolean powered:List.of(false,true))for(var event:List.of(GameEvent.STEP,GameEvent.BLOCK_PLACE,GameEvent.ENTITY_DIE,GameEvent.SHRIEK)) {
                var system=system(level,block.defaultBlockState());
                level.setBlock(POS.east(),blocked?Blocks.WOOL.pick(net.minecraft.world.item.DyeColor.WHITE).defaultBlockState():Blocks.AIR.defaultBlockState(),816);
                level.setBlock(POS.south(),powered?Blocks.REDSTONE_BLOCK.defaultBlockState():Blocks.AIR.defaultBlockState(),816);
                var origin=Vec3.atCenterOf(same?POS:POS.east(2));
                var context=GameEvent.Context.of(wool?Blocks.WOOL.pick(net.minecraft.world.item.DyeColor.WHITE).defaultBlockState():Blocks.STONE.defaultBlockState());
                boolean accepted=((GameEventListener.Provider<?>)system).getListener().handleGameEvent(level,event,context,origin);
                lines.add("listen\t"+Block.getId(block.defaultBlockState())+"\t"+same+"\t"+wool+"\t"+blocked+"\t"+powered+"\t"+event.unwrapKey().orElseThrow().identifier()+"\t"+accepted+"\t"+system.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            }
            level.setBlock(POS.east(),Blocks.AIR.defaultBlockState(),816);level.setBlock(POS.south(),Blocks.AIR.defaultBlockState(),816);
            for(boolean eligible:List.of(false,true))for(int charge:new int[]{0,1,50,1200})for(int calls:new int[]{1,3}) {
                tickingOverride=eligible;
                for(int x=4;x<=12;x++)for(int z=4;z<=12;z++)for(int y=79;y<=83;y++)level.setBlock(new BlockPos(x,y,z),y==79?Blocks.STONE.defaultBlockState():Blocks.AIR.defaultBlockState(),816);
                level.removeBlockEntity(POS);level.setBlock(POS,Blocks.SCULK_CATALYST.defaultBlockState(),3);
                var catalyst=(SculkCatalystBlockEntity)level.getBlockEntity(POS);
                catalyst.getListener().getSculkSpreader().addCursors(POS.east(2),charge);
                level.getRandom().setSeed(1234);
                for(int i=0;i<calls;i++)SculkCatalystBlockEntity.serverTick(level,POS,level.getBlockState(POS),catalyst);
                var blocks=new ArrayList<String>();
                for(int x=4;x<=12;x++)for(int y=79;y<=83;y++)for(int z=4;z<=12;z++) {
                    var position=new BlockPos(x,y,z);var actual=level.getBlockState(position);
                    var initial=y==79?Blocks.STONE.defaultBlockState():position.equals(POS)?Blocks.SCULK_CATALYST.defaultBlockState():Blocks.AIR.defaultBlockState();
                    if(actual!=initial)blocks.add(x+","+y+","+z+","+Block.getId(actual));
                }
                lines.add("catalyst\t"+eligible+"\t"+charge+"\t"+calls+"\t"+catalyst.saveCustomOnly(level.registryAccess())+"\t"+String.join(";",blocks)+"\t"+level.getRandom().nextLong());
            }
            tickingOverride=false;
            // These are native add/register and remove/unregister paths, not direct
            // calls to the listener; retained old objects remain observable.
            var sensor=(SculkSensorBlockEntity)system(level,Blocks.SCULK_SENSOR.defaultBlockState());
            level.gameEvent(GameEvent.BLOCK_PLACE,Vec3.atCenterOf(POS.east(2)),GameEvent.Context.of(Blocks.STONE.defaultBlockState()));
            lines.add("lifecycle\tplace\t"+sensor.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            sensor.getVibrationData().getSelectionStrategy().startOver();
            level.removeBlockEntity(POS);
            level.gameEvent(GameEvent.BLOCK_PLACE,Vec3.atCenterOf(POS.east(2)),GameEvent.Context.of(Blocks.STONE.defaultBlockState()));
            lines.add("lifecycle\tremove\t"+sensor.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            sensor=(SculkSensorBlockEntity)system(level,Blocks.SCULK_SENSOR.defaultBlockState());
            level.destroyBlock(POS,false);
            level.gameEvent(GameEvent.BLOCK_PLACE,Vec3.atCenterOf(POS.east(2)),GameEvent.Context.of(Blocks.STONE.defaultBlockState()));
            lines.add("lifecycle\tbreak\t"+sensor.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            var chunk=level.getChunkAt(POS);
            level.setBlock(POS,Blocks.SCULK_SENSOR.defaultBlockState(),816);
            level.removeBlockEntity(POS);
            var loaded=new SculkSensorBlockEntity(POS,Blocks.SCULK_SENSOR.defaultBlockState());
            chunk.setBlockEntity(loaded);
            level.gameEvent(GameEvent.BLOCK_PLACE,Vec3.atCenterOf(POS.east(2)),GameEvent.Context.of(Blocks.STONE.defaultBlockState()));
            lines.add("lifecycle\tbefore-load-register\t"+loaded.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            chunk.registerAllBlockEntitiesAfterLevelLoad();
            level.gameEvent(GameEvent.BLOCK_PLACE,Vec3.atCenterOf(POS.east(2)),GameEvent.Context.of(Blocks.STONE.defaultBlockState()));
            lines.add("lifecycle\tload-register\t"+loaded.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            loaded.getVibrationData().getSelectionStrategy().startOver();
            var replacement=new SculkSensorBlockEntity(POS,Blocks.SCULK_SENSOR.defaultBlockState());
            chunk.addAndRegisterBlockEntity(replacement);
            level.gameEvent(GameEvent.BLOCK_PLACE,Vec3.atCenterOf(POS.east(2)),GameEvent.Context.of(Blocks.STONE.defaultBlockState()));
            lines.add("lifecycle\treplacement\t"+loaded.isRemoved()+","+loaded.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent()+","+replacement.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            replacement.getVibrationData().getSelectionStrategy().startOver();
            var registry=chunk.getListenerRegistry(POS.getY()>>4);
            level.unload(chunk);
            var eventPos=Vec3.atCenterOf(POS.east(2));var context=GameEvent.Context.of(Blocks.STONE.defaultBlockState());
            registry.visitInRangeListeners(GameEvent.BLOCK_PLACE,eventPos,context,(listener,position)->listener.handleGameEvent(level,GameEvent.BLOCK_PLACE,context,eventPos));
            lines.add("lifecycle\tcaptured-unload\t"+replacement.isRemoved()+","+replacement.getVibrationData().getSelectionStrategy().chosenCandidate(Long.MAX_VALUE).isPresent());
            Files.write(output,lines);Files.writeString(output.resolveSibling("oracle-done"),"done\n");
        } catch(Throwable error) {error.printStackTrace();try {Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString());}catch(Exception ignored){}}
        finally {level.getServer().halt(false);}
    }
}
