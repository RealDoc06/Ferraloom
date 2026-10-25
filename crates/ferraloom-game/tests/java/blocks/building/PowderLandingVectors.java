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
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.FallingBlockEntity;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.storage.LevelStorageSource;

/** ConcretePowderBlock's real post-placement landing callback, including every
 * adjacent-water mask and waterlogged replaced states. */
public final class PowderLandingVectors extends AnvilVectors {
    final List<Integer> mutations = new ArrayList<>();
    PowderLandingVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) { super(server, storage); }
    @Override public void sendBlockUpdated(BlockPos pos,BlockState old,BlockState state,int flags) {
        if(pos.equals(POS)) {
            if(flags!=3)throw new AssertionError("onLand update flags: "+flags);
            mutations.add(Block.getId(state));
        }
    }
    void landing() {
        for(var block:BuiltInRegistries.BLOCK) if(block instanceof ConcretePowderBlock) {
            var state=block.defaultBlockState();
            var waterlogged=Blocks.OAK_FENCE.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true);
            var flowing=Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8);
            for(var replaced:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),flowing,waterlogged,state)) for(int mask=0;mask<64;mask++) {
                prepare(state,Blocks.AIR.defaultBlockState());
                for(var direction:Direction.values())getChunkAt(POS).setBlockState(POS.relative(direction),(mask&(1<<direction.get3DDataValue()))==0?Blocks.AIR.defaultBlockState():Blocks.WATER.defaultBlockState(),816);
                mutations.clear();
                var entity=new FallingBlockEntity(EntityTypes.FALLING_BLOCK,this);
                ((Fallable)block).onLand(this,POS,state,replaced,entity);
                out("powder-land",""+Block.getId(state),""+Block.getId(replaced),""+mask,""+Block.getId(getBlockState(POS)),mutations.toString());
            }
        }
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"powder-landing")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        new PowderLandingVectors(server,storage).landing();
        System.exit(0);
    }
}
final class PowderLandingCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors);
        PowderLandingVectors.run();
    }
}
