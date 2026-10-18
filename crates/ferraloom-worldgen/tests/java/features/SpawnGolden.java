import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.lang.classfile.*;
import java.lang.constant.*;
import java.lang.classfile.instruction.InvokeInstruction;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.PlayerSpawnFinder;
import net.minecraft.world.level.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.phys.*;

/** Real origin sampling and the private vanilla height-fixup algorithm. */
public final class SpawnGolden {
    static JsonArray requests;
    static ChunkPos searchOrigin;
    static int searchHeight, hitIndex;
    static BlockPos searchResult;
    static JsonArray visited;
    public static ChunkPos origin() { return searchOrigin; }
    public static int height() { return searchHeight; }
    public static void captureSpawn(Object ignored,net.minecraft.world.level.storage.LevelData.RespawnData data) { searchResult=data.pos(); }
    public static BlockPos captureChunk(net.minecraft.server.level.ServerLevel ignored,ChunkPos pos) {
        int index=visited.size();visited.add(FeaturesGolden.values(pos.x(),pos.z()));
        return index==hitIndex?new BlockPos(pos.getMinBlockX()+1,65,pos.getMinBlockZ()+2):null;
    }
    /** Keep the real initial-spawn bytecode; inject only world queries and the
     * chunk checker so this isolates origin, height, spiral and chosen position. */
    static Class<?> capturedInitialSpawn() throws Exception {
        var cf=ClassFile.of();
        byte[] bytes=net.minecraft.server.MinecraftServer.class.getResourceAsStream("MinecraftServer.class").readAllBytes();
        var method=cf.parse(bytes).methods().stream().filter(m->m.methodName().equalsString("setInitialSpawn")).findFirst().orElseThrow();
        byte[] transformed=cf.build(ClassDesc.of("CapturedInitialSpawn"),builder->builder.withMethod("invoke",method.methodTypeSymbol(),ClassFile.ACC_PUBLIC|ClassFile.ACC_STATIC,
            mb->mb.transformCode(method.code().orElseThrow(),(code,element)-> {
                if(element instanceof InvokeInstruction call) {
                    switch(call.name().stringValue()) {
                        case "getChunkSource", "getGenerator", "randomState" -> {code.pop().aconst_null();return;}
                        case "getOrigin" -> {code.pop2().invokestatic(ClassDesc.of("SpawnGolden"),"origin",MethodTypeDesc.of(ClassDesc.of("net.minecraft.world.level.ChunkPos")));return;}
                        case "getSpawnHeight" -> {code.pop2().invokestatic(ClassDesc.of("SpawnGolden"),"height",MethodTypeDesc.of(ConstantDescs.CD_int));return;}
                        case "getMinY" -> {code.pop().ldc(-64);return;}
                        case "getHeight" -> {code.pop2().pop2().ldc(92);return;}
                        case "dimension" -> {code.pop().getstatic(ClassDesc.of("net.minecraft.world.level.Level"),"OVERWORLD",ClassDesc.of("net.minecraft.resources.ResourceKey"));return;}
                        case "setSpawn" -> {code.invokestatic(ClassDesc.of("SpawnGolden"),"captureSpawn",MethodTypeDesc.of(ConstantDescs.CD_void,ConstantDescs.CD_Object,ClassDesc.of("net.minecraft.world.level.storage.LevelData$RespawnData")));return;}
                        case "getSpawnPosInChunk" -> {code.invokestatic(ClassDesc.of("SpawnGolden"),"captureChunk",call.typeSymbol());return;}
                    }
                }
                code.with(element);
            })));
        return new ClassLoader(SpawnGolden.class.getClassLoader()) {
            Class<?> define(){return defineClass("CapturedInitialSpawn",transformed,0,transformed.length);}
        }.define();
    }
    public static void captureCandidate(int x,int z,int index) { requests.add(FeaturesGolden.values(x,z,index)); }
    /** Replace only the asynchronous chunk-load boundary, leaving vanilla's
     * candidate selection bytecode intact. No transformed classes are saved. */
    static Class<?> capturedFinder() throws Exception {
        String name="net.minecraft.server.level.PlayerSpawnFinder";
        byte[] bytes=PlayerSpawnFinder.class.getResourceAsStream("PlayerSpawnFinder.class").readAllBytes();
        var cf=ClassFile.of();
        byte[] transformed=cf.transformClass(cf.parse(bytes),(builder,element)->{
            if(element instanceof MethodModel method && method.methodName().equalsString("scheduleCandidate")) {
                builder.withMethodBody(method.methodName(),method.methodType(),method.flags().flagsMask(),code->code
                    .iload(1).iload(2).iload(3).invokestatic(ClassDesc.of("SpawnGolden"),"captureCandidate",
                        MethodTypeDesc.of(ConstantDescs.CD_void,ConstantDescs.CD_int,ConstantDescs.CD_int,ConstantDescs.CD_int)).return_());
            } else builder.with(element);
        });
        return new ClassLoader(SpawnGolden.class.getClassLoader()) {
            Class<?> define(){return defineClass(name,transformed,0,transformed.length);}
        }.define();
    }
    static void generate(RegistryAccess.Frozen registries, Path out) throws Exception {
        var origins = new JsonArray();
        var initialCases=new JsonArray();
        var initial=capturedInitialSpawn().getDeclaredMethods()[0];
        var listener=(net.minecraft.server.level.progress.LevelLoadListener)Proxy.newProxyInstance(SpawnGolden.class.getClassLoader(),new Class<?>[]{net.minecraft.server.level.progress.LevelLoadListener.class},(proxy,method,args)->null);
        for (var preset : registries.lookupOrThrow(Registries.WORLD_PRESET).listElements().toList()) {
            var generator = preset.value().createWorldDimensions().overworld();
            for (long seed : FeaturesGolden.SEEDS) {
                var settings = generator instanceof NoiseBasedChunkGenerator noise ? noise.generatorSettings().value()
                    : registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
                var random = RandomState.create(registries.lookupOrThrow(Registries.NOISE), seed, settings);
                var origin = generator.getOrigin(random);
                var item = new JsonObject(); item.addProperty("preset",preset.key().identifier().toString());
                item.addProperty("seed",seed);item.add("origin",FeaturesGolden.values(origin.x(),origin.z()));origins.add(item);
                searchOrigin=origin;searchHeight=generator.getSpawnHeight(LevelHeightAccessor.create(-64,384));
                for(int hit:new int[]{0,1,2,3,17,120,-1}) {
                    hitIndex=hit;visited=new JsonArray();searchResult=null;
                    initial.invoke(null,null,null,false,false,listener);
                    var test=item.deepCopy();test.addProperty("hit",hit);test.addProperty("spawn_height",searchHeight);test.add("visited",visited);test.add("position",FeaturesGolden.values(searchResult.getX(),searchResult.getY(),searchResult.getZ()));initialCases.add(test);
                }
            }
        }
        WorldgenGolden.write(out.resolve("spawn_origins.json.gz"),origins);
        WorldgenGolden.write(out.resolve("initial_spawn_spiral.json.gz"),initialCases);
        var fixup = PlayerSpawnFinder.class.getDeclaredMethod("fixupSpawnHeight",CollisionGetter.class,BlockPos.class);
        fixup.setAccessible(true);var cases = new JsonArray();
        // CollisionGetter owns bounding-box and liquid semantics. This fixture
        // supplies a floor/ceiling, recording every real noCollision invocation.
        for(int floor:new int[]{-65,-64,0,63,318,319})for(int start:new int[]{-64,0,64,319,320})for(int ceiling:new int[]{-1,2,8}) {
            var calls = new JsonArray();
            var level=(CollisionGetter)Proxy.newProxyInstance(SpawnGolden.class.getClassLoader(),new Class<?>[]{CollisionGetter.class},(proxy,method,args)->switch(method.getName()) {
                case "getMinY" -> -64;
                case "getMaxY" -> 319;
                case "getHeight" -> 384;
                case "noCollision" -> {
                    if(args.length!=3||!Boolean.TRUE.equals(args[2]))throw new AssertionError("liquids must participate");
                    AABB box=(AABB)args[1];var call=new JsonArray();for(double v:new double[]{box.minX,box.minY,box.minZ,box.maxX,box.maxY,box.maxZ})call.add(v);calls.add(call);
                    yield box.minY>=floor+1 && (ceiling<0 || box.maxY<=floor+ceiling);
                }
                default -> throw new UnsupportedOperationException(method.toString());
            });
            var result=(Vec3)fixup.invoke(null,level,new BlockPos(-3,start,7));var item=new JsonObject();
            item.addProperty("floor",floor);item.addProperty("ceiling",ceiling);item.addProperty("start",start);
            var pos=new JsonArray();pos.add(result.x);pos.add(result.y);pos.add(result.z);item.add("position",pos);item.add("calls",calls);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("spawn_height_fixup.json.gz"),cases);
        var captured=capturedFinder();var constructor=captured.getDeclaredConstructor(net.minecraft.server.level.ServerLevel.class,BlockPos.class,int.class);constructor.setAccessible(true);
        var next=captured.getDeclaredMethod("scheduleNext");next.setAccessible(true);
        var offset=captured.getDeclaredField("offset");offset.setAccessible(true);
        var count=captured.getDeclaredField("candidateCount");count.setAccessible(true);
        var permutations=new JsonArray();
        for(long seed:FeaturesGolden.SEEDS)for(int radius:new int[]{0,1,2,3,16,32,128}) {
            var finder=constructor.newInstance(null,new BlockPos(-31,64,47),radius);
            var random=new net.minecraft.world.level.levelgen.LegacyRandomSource(seed);
            int size=count.getInt(finder);offset.setInt(finder,random.nextInt(size));requests=new JsonArray();
            for(int i=0;i<=size;i++)next.invoke(finder);
            var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("radius",radius);
            item.add("requests",requests);item.addProperty("next",random.nextLong());permutations.add(item);
        }
        WorldgenGolden.write(out.resolve("spawn_candidates.json.gz"),permutations);
        var faces=new JsonArray();
        for(var state:net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY) {
            try { faces.add(net.minecraft.world.level.block.Block.isFaceFull(state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO),Direction.UP)); }
            catch(Exception error) { faces.add(JsonNull.INSTANCE); }
        }
        WorldgenGolden.write(out.resolve("spawn_collision_faces.json.gz"),faces);
    }
}
