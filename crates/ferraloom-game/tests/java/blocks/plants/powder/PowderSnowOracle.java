import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import com.mojang.authlib.GameProfile;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.*;
import net.minecraft.core.*;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.phys.Vec3;

/** Real PowderSnowBlock calls on a dedicated ServerLevel. Only the separately
 * owned makeStuckInBlock virtual is a recording receiver for contact vectors. */
public final class PowderSnowOracle {
    static Path output;
    static final AtomicBoolean ran = new AtomicBoolean();
    static boolean recording;
    static String stuck = "-";
    static final List<String> emissions = new ArrayList<>();
    public static void premain(String argument, Instrumentation instrumentation) {
        output = Path.of(argument);
        new AgentBuilder.Default().disableClassFormatChanges()
            .with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .with(AgentBuilder.Listener.StreamWriting.toSystemError().withErrorsOnly())
            .type(named("net.minecraft.server.level.ServerLevel"))
            .transform((builder,type,loader,module,domain) -> builder
                .visit(Advice.to(OnTick.class).on(named("tick")))
                .visit(Advice.to(OnParticles.class).on(named("sendParticles").and(takesArguments(12))))
                .visit(Advice.to(OnSound.class).on(named("playSeededSound").and(takesArguments(9)))))
            .installOn(instrumentation);
    }
    public static class OnTick { @Advice.OnMethodEnter public static void before(@Advice.This Object level) { PowderSnowOracle.run(level); } }
    public static class OnParticles { @Advice.OnMethodEnter public static void before(@Advice.AllArguments Object[] args) { PowderSnowOracle.particles(args); } }
    public static class OnSound { @Advice.OnMethodEnter public static void before(@Advice.AllArguments Object[] args) { PowderSnowOracle.sound(args); } }
    static String d(double value) { return Long.toHexString(Double.doubleToRawLongBits(value)); }
    static String f(float value) { return Integer.toHexString(Float.floatToRawIntBits(value)); }
    static String vector(Vec3 value) { return d(value.x)+","+d(value.y)+","+d(value.z); }
    public static void particles(Object[] args) {
        if (recording && args[0] == ParticleTypes.SNOWFLAKE) {
            emissions.add("particle:"+vector(new Vec3((double)args[1],(double)args[2],(double)args[3]))
                +":"+args[4]+":"+f(((Double)args[8]).floatValue())+","+f(((Double)args[9]).floatValue())+","+f(((Double)args[10]).floatValue())
                +":"+((Enum<?>)args[11]).ordinal());
        }
    }
    public static void sound(Object[] args) {
        if (recording) {
            var sound = ((Holder<?>)args[4]).value();
            emissions.add("sound:"+BuiltInRegistries.SOUND_EVENT.getKey((net.minecraft.sounds.SoundEvent)sound)
                +":"+((Enum<?>)args[5]).ordinal()+":"+f((float)args[6])+":"+f((float)args[7]));
        }
    }
    static final class ProbeCow extends Cow {
        ProbeCow(ServerLevel level) { super(EntityTypes.COW,level); }
        @Override public void makeStuckInBlock(BlockState state,Vec3 multiplier) { stuck=vector(multiplier); }
    }
    static final class ProbePlayer extends ServerPlayer {
        ProbePlayer(ServerLevel level) { super(level.getServer(),level,new GameProfile(new UUID(0,1),"powder"),ClientInformation.createDefault()); }
        @Override public void makeStuckInBlock(BlockState state,Vec3 multiplier) { stuck=vector(multiplier); }
    }
    static final class ProbeItem extends ItemEntity {
        ProbeItem(ServerLevel level) { super(level,8.5,100,8.5,new ItemStack(Items.STONE)); }
        @Override public void makeStuckInBlock(BlockState state,Vec3 multiplier) { stuck=vector(multiplier); }
    }
    static Entity entity(ServerLevel level,int kind) {
        return switch(kind) { case 0 -> new ProbeItem(level); case 1 -> new ProbeCow(level); default -> new ProbePlayer(level); };
    }
    static String counters(Entity entity) { return entity.isInPowderSnow+","+entity.getTicksFrozen()+","+entity.getRemainingFireTicks(); }
    public static void run(Object input) {
        if (!ran.compareAndSet(false,true)) return;
        var level=(ServerLevel)input;
        try {
            var lines=new ArrayList<String>();
            var pos=new BlockPos(8,100,8);
            level.getChunkAt(pos);
            for(int kind=0;kind<3;kind++) for(boolean inSnow:List.of(false,true)) for(int fire:new int[]{-20,0,40})
            for(boolean grief:List.of(false,true)) for(boolean moving:List.of(false,true)) for(int seed:new int[]{0,1,1234}) {
                level.getGameRules().set(GameRules.MOB_GRIEFING,grief,level.getServer());
                level.getWorldBorder().setSize(kind==2 && seed==1234 ? 16 : 59999968);
                level.setBlock(pos,Blocks.POWDER_SNOW.defaultBlockState(),18);
                var entity=entity(level,kind);
                entity.setPos(inSnow ? 8.5 : 10.5,100,8.5);
                var movement=moving ? new Vec3(0.5,0,-0.25) : Vec3.ZERO;
                entity.setDeltaMovement(movement);
                if(entity instanceof ServerPlayer player) player.setKnownMovement(movement);
                entity.setRemainingFireTicks(fire);entity.setTicksFrozen(seed==1 ? 140 : 0);
                stuck="-";emissions.clear();recording=true;level.getRandom().setSeed(seed);
                var collector=new InsideBlockEffectApplier.StepBasedCollector();
                collector.advanceStep(0);
                Blocks.POWDER_SNOW.defaultBlockState().entityInside(level,pos,entity,collector,seed!=0);
                String queued=counters(entity);
                if(seed==1) level.setBlock(pos,Blocks.STONE.defaultBlockState(),18);
                collector.applyAndClear(entity);
                recording=false;
                lines.add("inside\t"+kind+"\t"+inSnow+"\t"+fire+"\t"+grief+"\t"+moving+"\t"+seed+"\t"+queued
                    +"\t"+stuck+"\t"+String.join(";",emissions)+"\t"+Block.getId(level.getBlockState(pos))+"\t"+counters(entity)+"\t"+level.getRandom().nextLong());
            }
            level.getWorldBorder().setSize(59999968);
            for(int kind=0;kind<3;kind++) for(double distance:new double[]{-1,0,Math.nextDown(4.0),4,Math.nextDown(7.0),7,128,Double.NaN,Double.POSITIVE_INFINITY})
            for(boolean silent:List.of(false,true)) {
                var entity=entity(level,kind);entity.setPos(8.5,100,8.5);entity.setSilent(silent);
                emissions.clear();recording=true;
                float before=entity instanceof LivingEntity living ? living.getHealth() : -1;
                Blocks.POWDER_SNOW.fallOn(level,Blocks.POWDER_SNOW.defaultBlockState(),pos,entity,distance);
                float after=entity instanceof LivingEntity living ? living.getHealth() : -1;
                recording=false;
                lines.add("fall\t"+kind+"\t"+d(distance)+"\t"+silent+"\t"+f(before)+"\t"+f(after)+"\t"+String.join(";",emissions));
            }
            Files.write(output,lines);Files.writeString(output.resolveSibling("oracle-done"),"done\n");
        } catch(Throwable error) {
            error.printStackTrace();try { Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString()); } catch(Exception ignored) {}
        } finally { level.getServer().halt(false); }
    }
}
