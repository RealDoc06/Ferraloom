import com.google.common.collect.ImmutableList;
import com.google.gson.*;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.*;
import static net.minecraft.world.level.block.Blocks.*;
import net.minecraft.core.*;
import net.minecraft.server.level.*;
import net.minecraft.util.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.chunk.status.*;

/** Actual ServerLevel and WorldGenRegion callbacks on disposable native chunks.
 * Reflection supplies cursor codec fields and completes fixture chunk futures;
 * no block method or spreader algorithm is replaced. */
public final class SculkOracle {
    static Path output;
    static final AtomicBoolean ran=new AtomicBoolean();
    static final Gson GSON=new Gson();
    static final Comparator<BlockPos> ORDER=Comparator.<BlockPos>comparingInt(BlockPos::getX).thenComparingInt(BlockPos::getY).thenComparingInt(BlockPos::getZ);
    static final List<String> events=new ArrayList<>();
    static boolean recording;
    static BlockPos origin;
    static final Field UPDATE=field(SculkSpreader.ChargeCursor.class,"updateDelay");
    static final Field DECAY=field(SculkSpreader.ChargeCursor.class,"decayDelay");
    static final Field FACES=field(SculkSpreader.ChargeCursor.class,"facings");
    static Field field(Class<?> owner,String name) {try {var f=owner.getDeclaredField(name);f.setAccessible(true);return f;} catch(Exception e){throw new RuntimeException(e);}}
    public static void premain(String argument,Instrumentation instrumentation) {
        output=Path.of(argument);
        new AgentBuilder.Default().disableClassFormatChanges()
            .with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .with(AgentBuilder.Listener.StreamWriting.toSystemError().withErrorsOnly())
            .type(named("net.minecraft.server.level.ServerLevel"))
            .transform((builder,type,loader,module,domain)->builder
                .visit(Advice.to(OnTick.class).on(named("tick")))
                .visit(Advice.to(OnEvent.class).on(named("levelEvent").and(takesArguments(4)))))
            .installOn(instrumentation);
    }
    public static class OnTick { @Advice.OnMethodEnter public static void before(@Advice.This Object level) { SculkOracle.run(level); } }
    public static class OnEvent { @Advice.OnMethodEnter public static void before(@Advice.Argument(1) int id,@Advice.Argument(2) Object pos,@Advice.Argument(3) int data) { SculkOracle.event(id,(BlockPos)pos,data); } }
    public static void event(int id,BlockPos pos,int data) {if(recording&&id==3006)events.add((pos.getX()-origin.getX())+":"+(pos.getY()-origin.getY())+":"+(pos.getZ()-origin.getZ())+":"+data);}
    static JsonArray point(BlockPos p) {var a=new JsonArray();a.add(p.getX());a.add(p.getY());a.add(p.getZ());return a;}
    static JsonArray cursors(SculkSpreader s) {
        var result=new JsonArray();
        for(var c:s.getCursors()) {var v=point(c.getPos());v.add(c.getCharge());v.add(c.getDecayDelay());try{v.add(UPDATE.getInt(c));}catch(Exception e){throw new RuntimeException(e);}
            if(c.getFacingData()==null)v.add(JsonNull.INSTANCE);else v.add(MultifaceBlock.pack(c.getFacingData()));result.add(v);}
        return result;
    }
    static SculkSpreader.ChargeCursor cursor(BlockPos pos,int charge,int decay,int delay,Integer mask) throws Exception {
        var c=new SculkSpreader.ChargeCursor(pos,charge);DECAY.setInt(c,decay);UPDATE.setInt(c,delay);
        if(mask!=null)FACES.set(c,MultifaceBlock.unpack(mask.byteValue()));return c;
    }
    static final class Holder extends GenerationChunkHolder {
        Holder(ChunkPos pos,ProtoChunk chunk)throws Exception {super(pos);var f=GenerationChunkHolder.class.getDeclaredMethod("completeFuture",ChunkStatus.class,ChunkAccess.class);f.setAccessible(true);f.invoke(this,ChunkStatus.TERRAIN,chunk);}
        protected void addSaveDependency(CompletableFuture<?> future){}
        public int getTicketLevel(){return 0;}
        public int getQueueLevel(){return 0;}
    }
    record RegionFixture(WorldGenRegion region,Map<ChunkPos,ProtoChunk> chunks){}
    static RegionFixture region(ServerLevel live,BlockPos pos)throws Exception {
        var factory=PalettedContainerFactory.create(live.registryAccess());
        var chunks=new TreeMap<ChunkPos,ProtoChunk>(Comparator.comparingInt(ChunkPos::x).thenComparingInt(ChunkPos::z));
        var dependencies=new ChunkDependencies(ImmutableList.of(ChunkStatus.TERRAIN,ChunkStatus.TERRAIN,ChunkStatus.TERRAIN));
        var step=new ChunkStep(ChunkStatus.FEATURES,dependencies,dependencies,2,(context,ignored,cache,chunk)->CompletableFuture.completedFuture(chunk));
        var center=ChunkPos.containing(pos);
        var cache=StaticCache2D.<GenerationChunkHolder>create(center.x(),center.z(),2,(x,z)->{
            try {var cp=new ChunkPos(x,z);var c=new ProtoChunk(cp,UpgradeData.EMPTY,live,factory,null);c.setPersistedStatus(ChunkStatus.TERRAIN);chunks.put(cp,c);return new Holder(cp,c);}catch(Exception e){throw new RuntimeException(e);}
        });
        return new RegionFixture(new WorldGenRegion(live,cache,step,chunks.get(center)),chunks);
    }
    static Map<BlockPos,BlockState> initial(int mode,boolean server) {
        var states=new TreeMap<BlockPos,BlockState>(ORDER);
        for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++)for(int y=-3;y<=-1;y++)states.put(origin.offset(x,y,z),STONE.defaultBlockState());
        if(mode==1)for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++)for(int y=0;y<=1;y++)states.put(origin.offset(x,y,z),WATER.defaultBlockState());
        if(server||mode==2||mode==3)for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++)states.put(origin.offset(x,1,z),STONE.defaultBlockState());
        if(mode==4)for(int z=-2;z<=2;z++)for(int y=0;y<=2;y++)states.put(origin.offset(-1,y,z),STONE.defaultBlockState());
        if(mode==2||mode==3||mode==4)for(int x=-2;x<=2;x++)for(int z=-2;z<=2;z++)states.put(origin.offset(x,0,z),SCULK.defaultBlockState());
        if(mode==7)for(int x=4;x<=8;x++)for(int z=-2;z<=2;z++)states.put(origin.offset(x,0,z),SCULK.defaultBlockState());
        return states;
    }
    static void setup(LevelAccessor level,Map<BlockPos,BlockState> states,boolean server) {
        for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++)for(int y=-5;y<=5;y++) {
            var p=origin.offset(x,y,z);var state=states.getOrDefault(p,AIR.defaultBlockState());
            // Direct section writes define the baseline without lifecycle callbacks.
            if(server) {var c=((ServerLevel)level).getChunkAt(p);c.getSection(c.getSectionIndex(p.getY())).setBlockState(p.getX()&15,p.getY()&15,p.getZ()&15,state,false);}
            else level.setBlock(p,state,816);
        }
    }
    static JsonArray blocks(LevelAccessor level,Map<BlockPos,BlockState> before) {
        var result=new JsonArray();
        for(int x=-16;x<=16;x++)for(int y=-5;y<=5;y++)for(int z=-16;z<=16;z++) {
            var p=origin.offset(x,y,z);var state=level.getBlockState(p);
            if(state!=before.getOrDefault(p,AIR.defaultBlockState())) {var v=point(p);v.add(Block.getId(state));result.add(v);}
        } return result;
    }
    static void regionOutput(JsonObject c,RegionFixture fixture) {
        var post=new ArrayList<String>();var entities=new TreeMap<BlockPos,String>(ORDER);
        for(var entry:fixture.chunks().entrySet()) {
            var cp=entry.getKey();var chunk=entry.getValue();var lists=chunk.getPostProcessing();
            for(int section=0;section<lists.length;section++)if(lists[section]!=null)for(short packed:lists[section]) {
                var p=ProtoChunk.unpackOffsetCoordinates(packed,chunk.getSectionYFromSectionIndex(section),cp);post.add(p.getX()+":"+p.getY()+":"+p.getZ());
            }
            for(var p:chunk.getBlockEntitiesPos())entities.put(p,chunk.getBlockEntityNbt(p).toString());
        }
        Collections.sort(post);c.add("post",GSON.toJsonTree(post));var bes=new JsonArray();
        for(var e:entities.entrySet()){var v=point(e.getKey());v.add(e.getValue());bes.add(v);}c.add("block_entities",bes);
    }
    public static void run(Object input) {
        if(!ran.compareAndSet(false,true))return;
        var live=(ServerLevel)input;
        try {
            live.getGameRules().set(net.minecraft.world.level.gamerules.GameRules.BLOCK_DROPS,false,live.getServer());
            live.getChunkSource().addTicketWithRadius(TicketType.FORCED,new ChunkPos(0,0),2);
            var updates=ServerChunkCache.class.getDeclaredMethod("runDistanceManagerUpdates");updates.setAccessible(true);updates.invoke(live.getChunkSource());
            var lines=new ArrayList<String>();
            origin=new BlockPos(8,100,8);
            for(int amount:new int[]{-1,0,1,999,1000,1001,31999,32000,32001,50000}) {
                var s=SculkSpreader.createWorldGenSpreader();s.addCursors(origin,amount);var c=new JsonObject();c.addProperty("kind","add");c.addProperty("amount",amount);c.add("cursors",cursors(s));lines.add(GSON.toJson(c));
            }
            for(int kind=0;kind<3;kind++)for(int mode=0;mode<8;mode++)for(int seed=0;seed<8;seed++) {
                boolean server=kind==2;origin=new BlockPos(seed%2==0?8:15,100,seed%2==0?8:15);
                RegionFixture fixture=server?null:region(live,origin);
                LevelAccessor level=server?live:fixture.region();
                var before=initial(mode,server);setup(level,before,server);
                var spreader=kind==1?SculkSpreader.createLevelSpreader():server?SculkSpreader.createLevelSpreader():SculkSpreader.createWorldGenSpreader();
                int calls=mode==6?1:mode==3?1:seed%3==0?1:seed%3==1?8:24;
                if(mode==3) {spreader.getCursors().add(cursor(origin,350,1,3,1));spreader.getCursors().add(cursor(origin,400,0,1,2));spreader.getCursors().add(cursor(origin,250,1,5,4));}
                else if(mode==6) {spreader.getCursors().add(cursor(origin.offset(1025,0,0),50,1,100,1));spreader.getCursors().add(cursor(origin.offset(1024,0,0),70,1,100,2));spreader.getCursors().add(cursor(origin,0,1,0,null));}
                else {spreader.addCursors(mode==7?origin.east(6):origin,seed%2==0?200:1501);if(mode==5)FACES.set(spreader.getCursors().getFirst(),seed%3==0?null:MultifaceBlock.unpack((byte)(seed%3==1?0:1)));}
                var c=new JsonObject();c.addProperty("kind",server?"server":kind==1?"region-level":"region-worldgen");c.addProperty("mode",mode);c.addProperty("tickable",server&&live.shouldTickBlocksAt(origin));c.addProperty("seed",seed);c.addProperty("calls",calls);c.addProperty("spread",seed%2==0||mode==7);c.add("origin",point(origin));c.add("before",cursors(spreader));
                boolean canonical=seed%4==0;
                level.getRandom().setSeed(canonical?seed:123);
                RandomSource random=canonical?level.getRandom():new LegacyRandomSource(seed);
                c.addProperty("canonical",canonical);c.addProperty("level_random",level.getRandom().getClass().getSimpleName());
                if(server&&!live.shouldTickBlocksAt(origin))throw new IllegalStateException("native forced ticket did not enable block ticking");
                events.clear();recording=true;
                for(int n=0;n<calls;n++)spreader.updateCursors(level,origin,random,c.get("spread").getAsBoolean());
                recording=false;c.add("cursors",cursors(spreader));c.add("blocks",blocks(level,before));c.add("events",GSON.toJsonTree(events));c.addProperty("next",random.nextLong());c.addProperty("level_next",level.getRandom().nextLong());
                if(!server)regionOutput(c,fixture);else {c.add("post",new JsonArray());c.add("block_entities",new JsonArray());}
                lines.add(GSON.toJson(c));
            }
            for(int mode:new int[]{0,1,4,7})for(int seed=0;seed<8;seed++)for(int config=0;config<2;config++) {
                origin=new BlockPos(seed%2==0?8:15,100,seed%2==0?8:15);var fixture=region(live,origin);var level=fixture.region();
                var before=initial(mode,false);setup(level,before,false);var random=new LegacyRandomSource(seed);
                level.getRandom().setSeed(123);var c=new JsonObject();c.addProperty("kind","patch");c.addProperty("mode",mode);c.addProperty("seed",seed);c.addProperty("canonical",false);c.addProperty("level_random",level.getRandom().getClass().getSimpleName());c.add("origin",point(origin));
                int charges=config==0?2:4,amount=config==0?50:200,attempts=config==0?16:32,growth=config==0?2:0,spread=config==0?2:3;
                c.addProperty("charges",charges);c.addProperty("amount",amount);c.addProperty("attempts",attempts);c.addProperty("growth",growth);c.addProperty("rounds",spread);
                var feature=new net.minecraft.world.level.levelgen.feature.SculkPatchFeature(charges,amount,attempts,growth,spread);
                c.addProperty("success",feature.place(level,live.getChunkSource().getGenerator(),random,origin));
                c.add("blocks",blocks(level,before));c.addProperty("next",random.nextLong());c.addProperty("level_next",level.getRandom().nextLong());regionOutput(c,fixture);lines.add(GSON.toJson(c));
            }
            origin=new BlockPos(8,100,8);var decayRegion=region(live,origin).region();
            decayRegion.setBlock(origin,SCULK.defaultBlockState(),816);decayRegion.setBlock(origin.above(),STONE.defaultBlockState(),816);
            for(boolean generation:new boolean[]{false,true})for(int distance:new int[]{0,1,3,4,12,24,25,1024})for(int charge:new int[]{0,1,49,50,999,1000})for(int seed=0;seed<8;seed++) {
                var spreader=generation?SculkSpreader.createWorldGenSpreader():SculkSpreader.createLevelSpreader();var cursor=new SculkSpreader.ChargeCursor(origin,charge);var random=new LegacyRandomSource(seed);
                int remaining=((SculkBehaviour)SCULK).attemptUseCharge(cursor,decayRegion,origin.east(distance),random,spreader,true);
                var c=new JsonObject();c.addProperty("kind","decay");c.addProperty("generation",generation);c.addProperty("distance",distance);c.addProperty("charge",charge);c.addProperty("seed",seed);c.addProperty("remaining",remaining);c.addProperty("next",random.nextLong());lines.add(GSON.toJson(c));
            }
            Files.write(output,lines);Files.writeString(output.resolveSibling("oracle-done"),"done\n");
        }catch(Throwable error){error.printStackTrace();try{Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString());}catch(Exception ignored){}}
        finally{live.getServer().halt(false);}
    }
}
