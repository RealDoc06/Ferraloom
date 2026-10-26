import com.google.gson.*;
import java.io.*;
import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import net.bytebuddy.ByteBuddy;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.NbtIo;
import net.minecraft.world.level.chunk.storage.SerializableChunkData;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.util.Util;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Observe the actual background bodies, not the synchronous future factories.
 * A controlled run holds only the first two entries at each asynchronous body;
 * executor identity, dependencies, method results and RNG draws stay native. */
public final class GenerationScheduleTrace {
    public static volatile boolean enabled;
    public static boolean gated;
    public static final AtomicLong IDS = new AtomicLong();
    public static final ConcurrentMap<String,AtomicInteger> ACTIVE = new ConcurrentHashMap<>();
    public static final ConcurrentMap<String,AtomicInteger> MAX = new ConcurrentHashMap<>();
    public static final ConcurrentMap<String,AtomicInteger> ENTERED = new ConcurrentHashMap<>();
    public static final ConcurrentMap<String,CountDownLatch> GATES = new ConcurrentHashMap<>();
    public static final ConcurrentLinkedQueue<JsonObject> EVENTS = new ConcurrentLinkedQueue<>();
    public record Frame(long id,String status,int x,int z,long start,String thread) {}
    public static Frame begin(String status,ChunkAccess chunk) {
        if(!enabled) return null;
        var pos=chunk.getPos();
        var frame=new Frame(IDS.incrementAndGet(),status,pos.x(),pos.z(),System.nanoTime(),Thread.currentThread().getName());
        int active=ACTIVE.computeIfAbsent(status,k->new AtomicInteger()).incrementAndGet();
        MAX.computeIfAbsent(status,k->new AtomicInteger()).accumulateAndGet(active,Math::max);
        int index=ENTERED.computeIfAbsent(status,k->new AtomicInteger()).incrementAndGet();
        if(gated && !status.equals("FEATURES") && index<=2) {
            var gate=GATES.computeIfAbsent(status,k->new CountDownLatch(2));
            gate.countDown();
            try {
                if(!gate.await(10,TimeUnit.SECONDS)) throw new AssertionError("native body did not overlap: "+status);
            } catch(InterruptedException error) { Thread.currentThread().interrupt(); throw new RuntimeException(error); }
        }
        return frame;
    }
    public static void end(Frame frame,Throwable failure) {
        if(frame==null) return;
        long end=System.nanoTime();
        ACTIVE.get(frame.status()).decrementAndGet();
        var row=new JsonObject();row.addProperty("ordinal",frame.id());row.addProperty("status",frame.status());
        row.addProperty("x",frame.x());row.addProperty("z",frame.z());row.addProperty("start_ns",frame.start());
        row.addProperty("end_ns",end);row.addProperty("thread",frame.thread());row.addProperty("failed",failure!=null);
        EVENTS.add(row);
    }
    public static void premain(String ignored,Instrumentation instrumentation) {
        new AgentBuilder.Default(new ByteBuddy().ignore(none())).disableClassFormatChanges()
            .ignore(none())
            .with(AgentBuilder.Listener.StreamWriting.toSystemError().withErrorsOnly())
            .type(named("net.minecraft.world.level.chunk.ChunkGenerator"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Biomes.class).on(named("doCreateBiomes"))))
            .type(named("net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Terrain.class).on(nameStartsWith("lambda$buildTerrain$"))))
            .type(named("net.minecraft.world.level.chunk.status.ChunkStatusTasks"))
            .transform((b,t,l,m,d)->b.visit(Advice.to(Features.class).on(named("generateFeatures"))))
            .installOn(instrumentation);
    }
    public static class Biomes {
        @Advice.OnMethodEnter public static Frame enter(@Advice.Argument(2) ChunkAccess chunk) { return begin("BIOMES",chunk); }
        @Advice.OnMethodExit(onThrowable=Throwable.class) public static void exit(@Advice.Enter Frame frame,@Advice.Thrown Throwable error) { end(frame,error); }
    }
    public static class Terrain {
        @Advice.OnMethodEnter public static Frame enter(@Advice.Argument(0) ChunkAccess chunk) { return begin("TERRAIN",chunk); }
        @Advice.OnMethodExit(onThrowable=Throwable.class) public static void exit(@Advice.Enter Frame frame,@Advice.Thrown Throwable error) { end(frame,error); }
    }
    public static class Features {
        @Advice.OnMethodEnter public static Frame enter(@Advice.Argument(3) ChunkAccess chunk) { return begin("FEATURES",chunk); }
        @Advice.OnMethodExit(onThrowable=Throwable.class) public static void exit(@Advice.Enter Frame frame,@Advice.Thrown Throwable error) { end(frame,error); }
    }
    static byte[] blocksHeightsAndNbt(net.minecraft.server.level.ServerLevel level)throws Exception {
        var bytes=new ByteArrayOutputStream();var output=new DataOutputStream(bytes);var position=new BlockPos.MutableBlockPos();
        for(int x=62;x<=66;x++)for(int z=62;z<=66;z++) {
            var chunk=level.getChunk(x,z,ChunkStatus.TERRAIN);
            for(int y=chunk.getMinY();y<=chunk.getMaxY();y++)for(int dz=0;dz<16;dz++)for(int dx=0;dx<16;dx++)
                output.writeInt(Block.BLOCK_STATE_REGISTRY.getId(chunk.getBlockState(position.set(x*16+dx,y,z*16+dz))));
            for(int dz=0;dz<16;dz++)for(int dx=0;dx<16;dx++) {
                output.writeInt(chunk.getHeight(Heightmap.Types.WORLD_SURFACE_WG,dx,dz));
                output.writeInt(chunk.getHeight(Heightmap.Types.OCEAN_FLOOR_WG,dx,dz));
            }
            NbtIo.write(SerializableChunkData.copyOf(level,chunk).write(),output);
        }
        return bytes.toByteArray();
    }
    public static void main(String[]args)throws Exception {
        var output=Path.of(args[0]);var variant=args[1];gated=variant.equals("gated");
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var temporary=Files.createTempDirectory("ferraloom-generation-schedule-");
        try {
            var source=LevelStorageSource.createDefault(temporary);var access=source.createAccess("world");var packs=ServerPacksSource.createPackRepository(access);
            try(var server=new PublishedLightTrace.Server(access,packs,PublishedLightTrace.load(packs))) {
                var level=server.getLevel(Level.OVERWORLD);
                enabled=true;
                if(variant.equals("observed") || variant.equals("gated")) {
                    level.getChunk(64,64,ChunkStatus.FULL);
                } else {
                    var cache=level.getChunkSource();
                    var request=cache.getClass().getDeclaredMethod("getChunkFutureMainThread",int.class,int.class,ChunkStatus.class,boolean.class);
                    request.setAccessible(true);
                    if(!variant.equals("serial")) {
                        // Queue native futures without the public wrapper's
                        // immediate managedBlock, then use native reads to wait.
                        for(int x=62;x<=66;x++)for(int z=62;z<=66;z++) request.invoke(cache,x,z,ChunkStatus.TERRAIN,true);
                    }
                    for(int x=62;x<=66;x++)for(int z=62;z<=66;z++) level.getChunk(x,z,ChunkStatus.TERRAIN);
                    enabled=false;
                    Files.write(output.resolveSibling(output.getFileName()+".terrain"),blocksHeightsAndNbt(level));
                    enabled=true;
                    var positions=new ArrayList<int[]>();
                    for(int x=63;x<=65;x++)for(int z=63;z<=65;z++)positions.add(new int[]{x,z});
                    if(variant.equals("permuted")) positions.addFirst(positions.remove(2));
                    for(var pos:positions)level.getChunk(pos[0],pos[1],ChunkStatus.FEATURES);
                }
                enabled=false;
                Files.write(output.resolveSibling(output.getFileName()+".blocks"),blocksHeightsAndNbt(level));
            }
        } finally {
            var rows=EVENTS.stream().sorted(Comparator.comparingLong(r->r.get("ordinal").getAsLong())).map(new Gson()::toJson).toList();
            Files.write(output,rows);
            try(var paths=Files.walk(temporary)) {for(var path:paths.sorted(Comparator.reverseOrder()).toList())Files.deleteIfExists(path);}
            Util.shutdownExecutors();
        }
    }
}
