import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.Vec3;

/** Actual cauldron precipitation virtuals on a real ServerLevel. The helper
 * captures exercise the protected native method directly, including NONE's
 * zero-draw path and first draws adjacent to both fill probability thresholds. */
final class CauldronVectors extends AnvilVectors {
    final List<String> trace = new ArrayList<>();
    boolean observe;
    CauldronVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) { super(server,storage); }
    @Override public void sendBlockUpdated(BlockPos p,BlockState old,BlockState state,int flags) {
        if(observe) {
            if(!p.equals(POS)||flags!=3)throw new AssertionError("unexpected block update");
            trace.add("B:"+Block.getId(old)+">"+Block.getId(state));
        }
    }
    @Override public void gameEvent(Holder<GameEvent> event,Vec3 pos,GameEvent.Context context) {
        if(observe) {
            if(!pos.equals(Vec3.atCenterOf(POS))||context.sourceEntity()!=null)throw new AssertionError("unexpected event context");
            trace.add("G:"+BuiltInRegistries.GAME_EVENT.getId(event.value())+","+(context.affectedState()==null?-1:Block.getId(context.affectedState())));
        }
    }
    static List<Long> seeds() {
        var seeds=new ArrayList<Long>();
        for(int i=0;i<256;i++)seeds.add(i*0x9e3779b97f4a7c15L);
        // Invert the LegacyRandomSource LCG for the nearest realizable 24-bit
        // first draw below/above RAIN_FILL_CHANCE and POWDER_SNOW_FILL_CHANCE.
        seeds.addAll(List.of(111365627497668L,53163938467012L,57542724734148L,280813797821636L));
        return seeds;
    }
    @Override void capture()throws Exception {
        var helper=CauldronBlock.class.getDeclaredMethod("shouldHandlePrecipitation",Level.class,Biome.Precipitation.class);helper.setAccessible(true);
        for(var precipitation:Biome.Precipitation.values())for(long seed:seeds()) {
            random.setSeed(seed);
            boolean result=(boolean)helper.invoke(null,this,precipitation);
            out("chance",precipitation.name().toLowerCase(Locale.ROOT),""+seed,""+result,""+random.nextLong());
        }
        for(var block:List.of(Blocks.CAULDRON,Blocks.WATER_CAULDRON,Blocks.POWDER_SNOW_CAULDRON,Blocks.LAVA_CAULDRON))
            for(var state:block.getStateDefinition().getPossibleStates())for(var precipitation:Biome.Precipitation.values())for(long seed:seeds()) {
                observe=false;prepare(state,Blocks.STONE.defaultBlockState());trace.clear();random.setSeed(seed);observe=true;
                block.handlePrecipitation(state,this,POS,precipitation);observe=false;
                out("handle",""+Block.getId(state),precipitation.name().toLowerCase(Locale.ROOT),""+seed,
                    ""+Block.getId(getBlockState(POS)),String.join(";",trace),""+random.nextLong());
            }
    }
    static void run()throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"cauldron-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        new CauldronVectors(server,storage).capture();System.exit(0);
    }
}
final class CauldronCapture {
    public static void main(String[] args)throws Exception {
        var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);CauldronVectors.run();
    }
}
