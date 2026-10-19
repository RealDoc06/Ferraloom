import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.named;
import net.minecraft.core.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.gameevent.*;
import net.minecraft.world.phys.Vec3;

/** Real GameEventDispatcher and registry callbacks on a disposable ServerLevel. */
public final class GameEventOracle {
    static Path output;
    static final AtomicBoolean ran=new AtomicBoolean();
    public static void premain(String argument,Instrumentation instrumentation) {
        output=Path.of(argument);
        new AgentBuilder.Default().disableClassFormatChanges()
            .with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .type(named("net.minecraft.server.level.ServerLevel"))
            .transform((builder,type,loader,module,domain)->builder.visit(Advice.to(OnTick.class).on(named("tick"))))
            .installOn(instrumentation);
    }
    public static class OnTick { @Advice.OnMethodEnter public static void before(@Advice.This Object level) { GameEventOracle.run(level); } }
    static class Listener implements GameEventListener {
        final String name;final BlockPos pos;final int radius;final DeliveryMode mode;final List<String> trace;
        Runnable action=()->{};
        Listener(String name,BlockPos pos,int radius,DeliveryMode mode,List<String> trace) {this.name=name;this.pos=pos;this.radius=radius;this.mode=mode;this.trace=trace;}
        public PositionSource getListenerSource(){return new BlockPositionSource(pos);}
        public int getListenerRadius(){return radius;}
        public DeliveryMode getDeliveryMode(){return mode;}
        public boolean handleGameEvent(ServerLevel level,Holder<GameEvent> event,GameEvent.Context context,Vec3 source) {
            if(context.affectedState()!=Blocks.STONE.defaultBlockState())throw new AssertionError("lost affected block context");
            trace.add(name);var next=action;action=()->{};next.run();return true;
        }
    }
    static Listener listener(ServerLevel level,List<String> trace,String name,BlockPos pos,int radius,boolean byDistance,boolean register) {
        var value=new Listener(name,pos,radius,byDistance?GameEventListener.DeliveryMode.BY_DISTANCE:GameEventListener.DeliveryMode.UNSPECIFIED,trace);
        if(register) registry(level,pos).register(value);return value;
    }
    static GameEventListenerRegistry registry(ServerLevel level,BlockPos pos){return level.getChunk(pos).getListenerRegistry(pos.getY()>>4);}
    static void emit(ServerLevel level,Holder<GameEvent> event,Vec3 pos){level.gameEvent(event,pos,GameEvent.Context.of(Blocks.STONE.defaultBlockState()));}
    static void expect(List<String> lines,String name,List<String> trace,String expected) {
        String actual=String.join(",",trace);trace.clear();if(!actual.equals(expected))throw new AssertionError(name+": "+actual+" != "+expected);lines.add(name+"\t"+actual);
    }
    public static void run(Object input) {
        if(!ran.compareAndSet(false,true))return;
        var level=(ServerLevel)input;
        try {
            var lines=new ArrayList<String>();var trace=new ArrayList<String>();level.getChunk(0,0);level.getChunk(1,0);
            var pos=new BlockPos(8,100,8);var source=Vec3.atCenterOf(pos);
            listener(level,trace,"C",pos.east(4),32,true,true);listener(level,trace,"A",pos,32,false,true);
            listener(level,trace,"D",pos.east(),32,true,true);listener(level,trace,"B",pos,32,false,true);listener(level,trace,"E",pos.west(),32,true,true);
            emit(level,GameEvent.STEP,source);expect(lines,"order",trace,"A,B,D,E,C");
            pos=new BlockPos(8,180,8);source=Vec3.atCenterOf(pos);
            listener(level,trace,"A",pos,8,false,true);listener(level,trace,"B",pos,7,false,true);
            emit(level,GameEvent.STEP,new Vec3(16.9,180.5,8.5));expect(lines,"block-distance",trace,"A");
            listener(level,trace,"C",new BlockPos(20,180,8),32,false,true);
            emit(level,GameEvent.JUKEBOX_PLAY,source);expect(lines,"section-bound",trace,"A,B,C");
            pos=new BlockPos(8,260,8);source=Vec3.atCenterOf(pos);
            var a=listener(level,trace,"A",pos,16,false,true);var b=listener(level,trace,"B",pos,16,false,true);
            registry(level,pos).register(b);listener(level,trace,"C",pos,16,false,true);var d=listener(level,trace,"D",pos,16,false,false);
            var reg=registry(level,pos);var duplicate=b;a.action=()->{reg.unregister(duplicate);reg.register(d);};
            emit(level,GameEvent.STEP,source);expect(lines,"deferred-duplicates-1",trace,"A,B,C");
            emit(level,GameEvent.STEP,source);expect(lines,"deferred-duplicates-2",trace,"A,B,C,D");
            pos=new BlockPos(8,340,8);source=Vec3.atCenterOf(pos);
            a=listener(level,trace,"A",pos.below(5),16,false,true);b=listener(level,trace,"B",pos,16,false,false);
            var laterRegistry=registry(level,pos);var later=b;a.action=()->laterRegistry.register(later);
            emit(level,GameEvent.STEP,source);expect(lines,"later-section",trace,"A,B");
            pos=new BlockPos(8,420,8);source=Vec3.atCenterOf(pos);
            a=listener(level,trace,"A",pos,16,false,true);listener(level,trace,"B",pos,16,false,true);var c=listener(level,trace,"C",pos,16,false,false);
            var nestedRegistry=registry(level,pos);var nestedSource=source;a.action=()->{nestedRegistry.register(c);emit(level,GameEvent.STEP,nestedSource);};
            try {emit(level,GameEvent.STEP,source);throw new AssertionError("missing native CME");}
            catch(ConcurrentModificationException expected){lines.add("exception\t"+expected.getClass().getName());}
            expect(lines,"nested-cme",trace,"A,A,B");
            emit(level,GameEvent.STEP,source);expect(lines,"after-cme",trace,"A,B,C");
            Files.write(output,lines);Files.writeString(output.resolveSibling("oracle-done"),"done\n");
        } catch(Throwable error) {error.printStackTrace();try {Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString());}catch(Exception ignored){}}
        finally {level.getServer().halt(false);}
    }
}
