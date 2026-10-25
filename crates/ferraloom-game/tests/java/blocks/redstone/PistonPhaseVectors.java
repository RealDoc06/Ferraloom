import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.piston.*;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Canonical state at ServerLevel tick boundaries. Actual runBlockEvents and
 * Level.tickBlockEntities execute; no piston/evaluator algorithm is copied.
 * Constructed chunks are explicitly block-ticking with loaded entity sections.
 * The broadcast boundary is sampled without a network peer, to distinguish
 * world-state changes from the later live packet-delivery observations. */
public final class PistonPhaseVectors extends RedstoneIdentityVectors {
    long time;
    PistonPhaseVectors(GameTestServer server,LevelStorageSource.LevelStorageAccess storage) { super(server,storage); }
    @Override public long getGameTime() { return time; }
    @Override public boolean shouldTickBlocksAt(long chunk) { return true; }
    @Override public boolean areEntitiesLoaded(long chunk) { return true; }
    String cell(BlockPos pos) {
        String moving="-";
        if(getBlockEntity(pos) instanceof PistonMovingBlockEntity entity) {
            moving=Float.floatToRawIntBits(entity.getProgress(0))+","+Float.floatToRawIntBits(entity.getProgress(1))+","+entity.getLastTicked()+","+entity.isExtending()+","+entity.isSourcePiston();
        }
        return Block.getId(getBlockState(pos))+":"+moving;
    }
    void snapshot(boolean sticky,int off,String phase) {
        System.out.println("PHASE\t"+sticky+"\t"+off+"\t"+time+"\t"+phase+"\t"+cell(POS)+"\t"+cell(POS.east())+"\t"+cell(POS.east(2)));
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"piston-phase")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var events=ServerLevel.class.getDeclaredMethod("runBlockEvents"); events.setAccessible(true);
        var handling=ServerLevel.class.getDeclaredField("handlingTick"); handling.setAccessible(true);
        for(boolean sticky:new boolean[]{false,true}) for(int off:new int[]{0,1,2,4}) {
            var world=new PistonPhaseVectors(server,storage);
            for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++)world.getChunkAt(new BlockPos(x*16,64,z*16)).setLoaded(true);
            var piston=(sticky?Blocks.STICKY_PISTON:Blocks.PISTON).defaultBlockState().setValue(PistonBaseBlock.FACING,Direction.EAST);
            world.getChunkAt(POS).setBlockState(POS,piston,816);
            world.getChunkAt(POS).setBlockState(POS.east(),Blocks.STONE.defaultBlockState(),816);
            world.setBlockAndUpdate(POS.west(),Blocks.REDSTONE_BLOCK.defaultBlockState());
            world.snapshot(sticky,off,"command-on");
            if(off==0) {
                world.setBlockAndUpdate(POS.west(),Blocks.AIR.defaultBlockState());
                world.snapshot(sticky,off,"command-off");
            }
            for(int tick=1;tick<=8;tick++) {
                server.tickRateManager().setFrozen(false); server.tickRateManager().tick();
                world.time=tick;
                handling.setBoolean(world,true);
                world.snapshot(sticky,off,"chunk-broadcast");
                events.invoke(world);
                world.snapshot(sticky,off,"block-events");
                handling.setBoolean(world,false);
                world.tickBlockEntities();
                world.snapshot(sticky,off,"block-entities");
                server.tickRateManager().setFrozen(true); server.tickRateManager().tick();
                world.tickBlockEntities();
                world.snapshot(sticky,off,"frozen-broadcast");
                if(tick==off) {
                    world.setBlockAndUpdate(POS.west(),Blocks.AIR.defaultBlockState());
                    world.snapshot(sticky,off,"command-off");
                }
            }
        }
        System.exit(0);
    }
}
final class PistonPhaseCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors);
        PistonPhaseVectors.run();
    }
}
