import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.attribute.*;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.Vec3;

/** Native PointedDripstoneBlock randomTick and its public testing entry points.
 * The real level performs all block mutations, neighbor updates and scheduling;
 * only externally visible writes/events and the final saved queues are recorded. */
final class DripstoneVectors extends AnvilVectors {
    static final int LOW = -24, HIGH = 3;
    final List<String> trace = new ArrayList<>();
    EnvironmentAttributeSystem environment;
    boolean observe;
    long subTick;
    DripstoneVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) { super(server,storage); }
    @Override public long nextSubTickCount() { return subTick++; }
    @Override public EnvironmentAttributeSystem environmentAttributes() { return environment == null ? super.environmentAttributes() : environment; }
    static String pos(BlockPos p) { return p.getX()+","+p.getY()+","+p.getZ(); }
    @Override public void sendBlockUpdated(BlockPos p,BlockState old,BlockState state,int flags) {
        if(observe) trace.add("B:"+pos(p)+":"+Block.getId(old)+">"+Block.getId(state));
    }
    @Override public void levelEvent(Entity entity,int event,BlockPos p,int data) {
        if(observe) trace.add("E:"+pos(p)+":"+event+","+data);
    }
    @Override public void gameEvent(Holder<GameEvent> event,Vec3 p,GameEvent.Context context) {
        if(observe) {
            if(context.sourceEntity()!=null)throw new AssertionError("unexpected game-event actor");
            trace.add("G:"+pos(BlockPos.containing(p))+":"+BuiltInRegistries.GAME_EVENT.getId(event.value())+","+(context.affectedState()==null?-1:Block.getId(context.affectedState())));
        }
    }
    record Scene(BlockState[] column,boolean evaporates) {}
    static BlockState spike(Block block,Direction dir,SpeleothemThickness thickness,boolean wet) {
        return block.defaultBlockState().setValue(SpeleothemBlock.TIP_DIRECTION,dir).setValue(SpeleothemBlock.THICKNESS,thickness).setValue(SpeleothemBlock.WATERLOGGED,wet);
    }
    static void put(BlockState[] column,int y,BlockState state) { column[y-LOW]=state; }
    static Scene scene(BlockState origin,int length,BlockState source,int gap,BlockState floor,BlockState barrier,boolean evaporates) {
        var column=new BlockState[HIGH-LOW+1];Arrays.fill(column,Blocks.AIR.defaultBlockState());
        put(column,0,origin);put(column,1,Blocks.DRIPSTONE_BLOCK.defaultBlockState());put(column,2,source);
        for(int i=1;i<length;i++)put(column,-i,spike(Blocks.POINTED_DRIPSTONE,Direction.DOWN,i==length-1?SpeleothemThickness.TIP:SpeleothemThickness.MIDDLE,false));
        int tip=1-length;
        put(column,tip-gap,floor);
        if(gap>1)put(column,tip-1,barrier);
        return new Scene(column,evaporates);
    }
    static List<Scene> scenes() {
        var result=new ArrayList<Scene>();
        for(var state:Blocks.POINTED_DRIPSTONE.getStateDefinition().getPossibleStates())
            for(int length:new int[]{1,2,6,7,10,11,12})
                for(var source:List.of(Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,1),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.LAVA.defaultBlockState(),Blocks.LAVA.defaultBlockState().setValue(LiquidBlock.LEVEL,8),Blocks.MUD.defaultBlockState(),Blocks.AIR.defaultBlockState())) {
                    result.add(scene(state,length,source,2,Blocks.CAULDRON.defaultBlockState(),Blocks.AIR.defaultBlockState(),false));
                    if(source.is(Blocks.MUD))result.add(scene(state,length,source,2,Blocks.CAULDRON.defaultBlockState(),Blocks.AIR.defaultBlockState(),true));
                }
        var tip=spike(Blocks.POINTED_DRIPSTONE,Direction.DOWN,SpeleothemThickness.TIP,false);
        var floors=List.of(Blocks.CAULDRON.defaultBlockState(),Blocks.WATER_CAULDRON.defaultBlockState(),Blocks.WATER_CAULDRON.defaultBlockState().setValue(LayeredCauldronBlock.LEVEL,3),Blocks.POWDER_SNOW_CAULDRON.defaultBlockState(),Blocks.LAVA_CAULDRON.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.AIR.defaultBlockState(),spike(Blocks.POINTED_DRIPSTONE,Direction.UP,SpeleothemThickness.TIP,false));
        var barriers=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.IRON_BARS.defaultBlockState(),Blocks.OAK_FENCE.defaultBlockState(),Blocks.OAK_TRAPDOOR.defaultBlockState().setValue(TrapDoorBlock.OPEN,true),Blocks.STONE_SLAB.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP),Blocks.CAULDRON.defaultBlockState(),Blocks.GLASS_PANE.defaultBlockState());
        for(int gap:new int[]{1,2,9,10,11,12})for(var floor:floors)for(var barrier:barriers)
            result.add(scene(tip,1,Blocks.WATER.defaultBlockState(),gap,floor,barrier,false));
        // Target predicates run before the same-block/direction path predicates.
        for(var other:List.of(Blocks.POINTED_DRIPSTONE,Blocks.SULFUR_SPIKE))for(var direction:List.of(Direction.UP,Direction.DOWN)) {
            var base=scene(spike(Blocks.POINTED_DRIPSTONE,Direction.DOWN,SpeleothemThickness.BASE,false),1,Blocks.MUD.defaultBlockState(),4,Blocks.CAULDRON.defaultBlockState(),Blocks.AIR.defaultBlockState(),false);
            put(base.column,-2,spike(other,direction,SpeleothemThickness.TIP,false));result.add(base);
        }
        return result;
    }
    void begin(Scene scene)throws Exception {
        observe=false;prepare(Blocks.AIR.defaultBlockState(),Blocks.AIR.defaultBlockState());subTick=0;
        for(int y=LOW;y<=HIGH;y++)getChunkAt(POS).setBlockState(POS.above(y),scene.column[y-LOW],816);
        environment=EnvironmentAttributeSystem.builder().addConstantLayer(EnvironmentAttributes.WATER_EVAPORATES,old->scene.evaporates).build();
        trace.clear();observe=true;
    }
    String column() { return IntStream.rangeClosed(LOW,HIGH).mapToObj(y->""+Block.getId(getBlockState(POS.above(y)))).collect(Collectors.joining(",")); }
    static <T> Stream<net.minecraft.world.ticks.SavedTick<T>> queued(net.minecraft.world.ticks.LevelTicks<T> scheduler)throws Exception {
        var field=net.minecraft.world.ticks.LevelTicks.class.getDeclaredField("allContainers");field.setAccessible(true);
        var containers=(it.unimi.dsi.fastutil.longs.Long2ObjectMap<net.minecraft.world.ticks.LevelChunkTicks<T>>)field.get(scheduler);
        return containers.get(net.minecraft.world.level.ChunkPos.pack(POS)).pack(0).stream();
    }
    String scheduled()throws Exception {
        var blocks=queued(ticks).map(t->"B:"+pos(t.pos())+":"+BuiltInRegistries.BLOCK.getId(t.type())+","+t.delay()+","+t.priority().getValue());
        var fluids=queued(fluidTicks).map(t->"F:"+pos(t.pos())+":"+BuiltInRegistries.FLUID.getId(t.type())+","+t.delay()+","+t.priority().getValue());
        return Stream.concat(blocks,fluids).collect(Collectors.joining(";"));
    }
    void sample(String mode,Scene scene,long seed,float value)throws Exception {
        begin(scene);var before=column();random.setSeed(seed);var state=getBlockState(POS);
        switch(mode) {
            case "random" -> state.randomTick(this,POS,random);
            case "transfer" -> PointedDripstoneBlock.maybeTransferFluid(state,this,POS,value);
            case "grow" -> ((SpeleothemBlock)Blocks.POINTED_DRIPSTONE).growStalactiteOrStalagmiteIfPossible(state,this,POS,random);
            default -> throw new AssertionError(mode);
        }
        observe=false;
        out(mode,before,""+scene.evaporates,""+seed,""+Float.floatToRawIntBits(value),column(),String.join(";",trace),scheduled(),""+random.nextLong());
    }
    @Override void capture()throws Exception {
        int index=0;
        for(var scene:scenes()) {
            for(float value:new float[]{0.0F,Math.nextDown(0.05859375F),0.05859375F,Math.nextUp(0.05859375F),Math.nextDown(0.17578125F),0.17578125F,Float.NaN,-0.0F})sample("transfer",scene,42,value);
            for(long seed:new long[]{0,4096})sample("grow",scene,seed,0);
            if(index++%16==0) {
                for(long seed=0;seed<32;seed++)sample("random",scene,seed,0);
                for(long seed:new long[]{42,4096,-1,Long.MIN_VALUE})sample("random",scene,seed,0);
            }
        }
    }
    static void run()throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"dripstone-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        new DripstoneVectors(server,storage).capture();System.exit(0);
    }
}
final class DripstoneCapture {
    public static void main(String[] args)throws Exception {
        var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);DripstoneVectors.run();
    }
}
