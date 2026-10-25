import com.google.gson.*;
import com.mojang.serialization.Lifecycle;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.*;
import java.net.Proxy;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicLong;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.*;
import net.minecraft.*;
import net.minecraft.commands.Commands;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.level.progress.LoggingLevelLoadListener;
import net.minecraft.server.notifications.*;
import net.minecraft.server.packs.repository.*;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.*;
import net.minecraft.util.Util;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.util.debugchart.*;
import net.minecraft.util.StaticCache2D;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.*;
import net.minecraft.world.level.dimension.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.lighting.LevelLightEngine;
import net.minecraft.world.level.storage.*;

/** Observe native published light at the precise Strict mushroom stop. The
 * only timing perturbation is a labeled delay before the light update body.
 * No Minecraft method result, feature, random draw or block write is replaced. */
public final class PublishedLightTrace {
    static final Gson JSON = new Gson();
    static final ConcurrentLinkedQueue<JsonObject> EVENTS = new ConcurrentLinkedQueue<>();
    static final AtomicLong SEQUENCE = new AtomicLong();
    static final ThreadLocal<Read> READ = new ThreadLocal<>();
    static final ThreadLocal<String> PROBE = new ThreadLocal<>();
    static final ConcurrentMap<Object,String> PUBLISHED = new ConcurrentHashMap<>();
    static volatile String phase = "SETUP";
    static volatile WorldGenRegion targetRegion;
    static long delayMillis;

    static final class Read {
        final WorldGenRegion region;
        final String role;
        Integer sky, block;
        Read(WorldGenRegion region, String role) { this.region=region; this.role=role; }
    }
    static boolean target(BlockPos p) { return p.getX()==153 && p.getY()==158 && p.getZ()==-1; }
    static BlockPos point() { return new BlockPos(153,158,-1); }
    static Object field(Object owner,String name) {
        for(Class<?> c=owner.getClass();c!=null;c=c.getSuperclass()) try {
            var f=c.getDeclaredField(name); f.setAccessible(true); return f.get(owner);
        } catch(NoSuchFieldException absent) { } catch(ReflectiveOperationException e) { throw new RuntimeException(e); }
        throw new IllegalArgumentException(name+" in "+owner.getClass());
    }
    static JsonObject event(String kind) {
        var row=new JsonObject(); row.addProperty("sequence",SEQUENCE.incrementAndGet());
        row.addProperty("kind",kind); row.addProperty("phase",phase);
        row.addProperty("nanos",System.nanoTime()); row.addProperty("thread",Thread.currentThread().getName());
        return row;
    }
    static JsonObject visible(Object storage) {
        var row=new JsonObject(); var map=field(storage,"visibleSectionData");
        row.addProperty("map_identity",System.identityHashCode(map));
        if(storage.getClass().getSimpleName().equals("SkyLightSectionStorage")) {
            var tops=(it.unimi.dsi.fastutil.longs.Long2IntMap)field(map,"topSections");
            row.addProperty("top",tops.get(SectionPos.getZeroNode(SectionPos.of(point()).asLong())));
            row.addProperty("lowest",(int)field(map,"currentLowestY"));
        }
        return row;
    }
    public static void premain(String ignored,Instrumentation instrumentation) {
        new AgentBuilder.Default().disableClassFormatChanges()
            .with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .with(AgentBuilder.Listener.StreamWriting.toSystemError().withErrorsOnly())
            .type(named("net.minecraft.world.level.block.MushroomBlock"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Mushroom.class).on(named("canSurvive"))))
            .type(named("net.minecraft.world.level.lighting.LevelLightEngine"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Raw.class).on(named("getRawBrightness"))))
            .type(named("net.minecraft.world.level.lighting.SkyLightSectionStorage"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Sky.class).on(named("getLightValue").and(takesArguments(2)))))
            .type(named("net.minecraft.world.level.lighting.BlockLightSectionStorage"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Block.class).on(named("getLightValue"))))
            .type(named("net.minecraft.world.level.lighting.LayerLightSectionStorage"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Publish.class).on(named("swapSectionMap"))))
            .type(named("net.minecraft.server.level.ThreadedLevelLightEngine"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Delay.class).on(named("runUpdate"))))
            .type(named("net.minecraft.world.level.chunk.status.ChunkStatusTasks"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Features.class).on(named("generateFeatures"))))
            .installOn(instrumentation);
    }
    public static class Mushroom {
        @Advice.OnMethodEnter public static Object enter(@Advice.Argument(1) Object level,@Advice.Argument(2) Object p) {
            return PublishedLightTrace.begin(level,(BlockPos)p);
        }
        @Advice.OnMethodExit(onThrowable=Throwable.class) public static void exit(@Advice.Enter Object before) {
            PublishedLightTrace.end(before);
        }
    }
    public static Object begin(Object level,BlockPos p) {
        var before=READ.get();
        if(level instanceof WorldGenRegion region && target(p))
            READ.set(new Read(region,Optional.ofNullable(PROBE.get()).orElse("natural")));
        return before;
    }
    public static void end(Object before) { if(before==null) READ.remove(); else READ.set((Read)before); }
    public static class Sky {
        @Advice.OnMethodExit public static void exit(@Advice.Argument(0) long p,@Advice.Argument(1) boolean updating,@Advice.Return int value) {
            if(!updating) PublishedLightTrace.layer(p,value,true);
        }
    }
    public static class Block {
        @Advice.OnMethodExit public static void exit(@Advice.Argument(0) long p,@Advice.Return int value) {
            PublishedLightTrace.layer(p,value,false);
        }
    }
    public static void layer(long p,int value,boolean sky) {
        var read=READ.get(); if(read!=null && target(BlockPos.of(p))) {
            if(sky) read.sky=value; else read.block=value;
        }
    }
    public static class Raw {
        @Advice.OnMethodExit public static void exit(@Advice.This Object engine,@Advice.Argument(0) Object p,@Advice.Argument(1) int dampen,@Advice.Return int value) {
            PublishedLightTrace.raw((LevelLightEngine)engine,(BlockPos)p,dampen,value);
        }
    }
    public static void raw(LevelLightEngine engine,BlockPos p,int dampen,int value) {
        var read=READ.get(); if(read==null || !target(p)) return;
        var row=event("raw_read"); row.addProperty("role",read.role);
        row.addProperty("center",read.region.getCenter().toString());
        var chunk=read.region.getChunk(p.getX()>>4,p.getZ()>>4,ChunkStatus.EMPTY);
        row.addProperty("chunk_status",chunk.getPersistedStatus().getName().toString());
        row.addProperty("sky",read.sky); row.addProperty("block",read.block);
        row.addProperty("dampen",dampen); row.addProperty("raw",value);
        for(String name:List.of("skyEngine","blockEngine")) {
            var layer=field(engine,name); if(layer!=null) row.add(name,visible(field(layer,"storage")));
        }
        EVENTS.add(row);
    }
    public static class Publish {
        @Advice.OnMethodEnter public static Object enter(@Advice.FieldValue("visibleSectionData") Object map) { return map; }
        @Advice.OnMethodExit public static void exit(@Advice.This Object storage,@Advice.Enter Object before,@Advice.FieldValue("visibleSectionData") Object after) {
            if(before!=after) PublishedLightTrace.publish(storage);
        }
    }
    public static void publish(Object storage) {
        var row=visible(storage); var signature=row.toString();
        if(signature.equals(PUBLISHED.put(storage,signature))) return;
        var event=event("publish"); event.addProperty("layer",field(storage,"layer").toString());
        event.add("visible",row); EVENTS.add(event);
    }
    public static class Delay { @Advice.OnMethodEnter public static void enter() { PublishedLightTrace.pause(); } }
    public static void pause() {
        if(phase.equals("FEATURES") && delayMillis>0) try { Thread.sleep(delayMillis); }
        catch(InterruptedException e) { Thread.currentThread().interrupt(); }
    }
    public static class Features {
        @Advice.OnMethodEnter public static boolean enter(@Advice.Argument(0) Object context,@Advice.Argument(1) Object step,@Advice.Argument(2) Object chunks,@Advice.Argument(3) Object chunk) {
            return PublishedLightTrace.features(context,step,chunks,(ChunkAccess)chunk);
        }
        @Advice.OnMethodExit public static void exit(@Advice.Enter boolean target) { if(target) PublishedLightTrace.probe("feature-exit"); }
    }
    @SuppressWarnings("unchecked")
    public static boolean features(Object context,Object step,Object chunks,ChunkAccess chunk) {
        if(chunk.getPos().x()!=9 || chunk.getPos().z()!=0) return false;
        targetRegion=new WorldGenRegion(((WorldGenContext)context).level(),(StaticCache2D<GenerationChunkHolder>)chunks,(ChunkStep)step,chunk);
        probe("feature-entry"); return true;
    }
    public static void probe(String role) {
        PROBE.set(role);
        try {
            boolean survives=Blocks.BROWN_MUSHROOM.defaultBlockState().canSurvive(targetRegion,point());
            var row=event("predicate_probe"); row.addProperty("role",role);
            row.addProperty("below",targetRegion.getBlockState(point().below()).toString());
            row.addProperty("survives",survives); EVENTS.add(row);
            // Probe the exact native getter even when the mushroom's substrate
            // tag short-circuits before the light query in this schedule.
            var before=READ.get(); READ.set(new Read(targetRegion,role+"-direct-light"));
            try { targetRegion.getRawBrightness(point(),0); }
            finally { end(before); }
        } finally { PROBE.remove(); }
    }
    static final class Server extends MinecraftServer {
        private final LocalSampleLogger logger=new LocalSampleLogger(4);
        Server(LevelStorageSource.LevelStorageAccess storage,PackRepository packs,WorldStem stem)throws Exception {
            super(Thread.currentThread(),storage,packs,stem,Optional.empty(),Proxy.NO_PROXY,DataFixers.getDataFixer(),noServices(),LoggingLevelLoadListener.forDedicatedServer(),false,new NotificationManager());
            setPlayerList(new PlayerList(this,registries(),playerDataStorage,new EmptyNotificationService()){}); createLevels();
        }
        static Services noServices()throws Exception { var f=GameTestServer.class.getDeclaredField("NO_SERVICES");f.setAccessible(true);return (Services)f.get(null); }
        @Override protected boolean initServer(){return true;}
        @Override public LevelBasedPermissionSet operatorUserPermissions(){return LevelBasedPermissionSet.ALL;}
        @Override public PermissionSet getFunctionCompilationPermissions(){return LevelBasedPermissionSet.OWNER;}
        @Override public boolean shouldRconBroadcast(){return false;}
        @Override protected SampleLogger getTickTimeLogger(){return logger;}
        @Override public boolean isTickTimeLoggingEnabled(){return false;}
        @Override public SystemReport fillServerSystemReport(SystemReport report){return report;}
        @Override public boolean isDedicatedServer(){return false;}
        @Override public int getMaxPlayers(){return 0;}
        @Override public int getRateLimitPacketsPerSecond(){return 0;}
        @Override public int getCommandSpamThresholdSeconds(){return 0;}
        @Override public int getChatSpamThresholdSeconds(){return 0;}
        @Override public boolean useNativeTransport(){return false;}
        @Override public boolean isPublished(){return false;}
        @Override public boolean shouldInformAdmins(){return false;}
        @Override public boolean isSingleplayerOwner(NameAndId id){return false;}
        @Override protected void waitUntilNextTick(){runAllTasks();}
    }
    static WorldStem load(PackRepository packs)throws Exception {
        packs.reload(); var config=new WorldLoader.InitConfig(new WorldLoader.PackConfig(packs,WorldDataConfiguration.DEFAULT,false,true),Commands.CommandSelection.DEDICATED,LevelBasedPermissionSet.OWNER);
        return Util.<WorldStem>blockUntilDone(executor->WorldLoader.load(config,context->{
            var dimensions=context.datapackWorldRegistries().lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.NORMAL).value().createWorldDimensions();
            var complete=dimensions.bake(new MappedRegistry<LevelStem>(Registries.LEVEL_STEM,Lifecycle.stable()).freeze());
            var settings=new LevelSettings("Published light trace",GameType.SURVIVAL,LevelSettings.DifficultySettings.DEFAULT,false,context.dataConfiguration());
            var data=new PrimaryLevelData(settings,complete.specialWorldProperty(),complete.lifecycle()); data.setInitialized(true);
            return new WorldLoader.DataLoadOutput<>(new LevelDataAndDimensions.WorldDataAndGenSettings(data,new WorldGenSettings(new WorldOptions(12345L,false,false),dimensions)),complete.dimensionsRegistryAccess());
        },WorldStem::new,Util.backgroundExecutor(),executor)).get();
    }
    public static void main(String[]args)throws Exception {
        delayMillis=Long.parseLong(args[1]); SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();
        Path output=Path.of(args[0]).toAbsolutePath(),temporary=Files.createTempDirectory("ferraloom-published-light-");
        try {
            var source=LevelStorageSource.createDefault(temporary); var access=source.createAccess("world"); var packs=ServerPacksSource.createPackRepository(access);
            try(var server=new Server(access,packs,load(packs))) {
                var level=server.getLevel(Level.OVERWORLD);
                phase="INITIALIZE_NEIGHBOR"; level.getChunk(9,-1,ChunkStatus.INITIALIZE_LIGHT);
                phase="FEATURES"; level.getChunk(9,0,ChunkStatus.FEATURES);
                if(targetRegion==null) throw new AssertionError("target FEATURES body was not observed");
                phase="DRAIN";
                var pending=level.getChunkSource().getLightEngine().waitForPendingTasks(9,-1);
                while(!pending.isDone()) { level.getChunkSource().getLightEngine().tryScheduleUpdate(); Thread.sleep(1); }
                pending.join(); probe("after-light-drain");
            }
        } finally {
            Files.write(output,EVENTS.stream().map(JSON::toJson).toList());
            try(var paths=Files.walk(temporary)) { for(var path:paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(path); }
            Util.shutdownExecutors();
        }
    }
}
