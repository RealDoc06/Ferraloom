import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Real BuddingAmethystBlock random ticks on a real ServerLevel. Only outbound
 * block notifications are observed; growth and neighbor updates remain native. */
final class AmethystVectors extends AnvilVectors {
    final List<String> writes = new ArrayList<>();
    AmethystVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) {
        super(server, storage);
    }
    @Override public void sendBlockUpdated(BlockPos pos, BlockState old, BlockState state, int flags) {
        writes.add(pos.getX()+","+pos.getY()+","+pos.getZ()+":"+Block.getId(state)+"/"+flags);
    }
    @Override void capture() {
        for (var block : BuiltInRegistries.BLOCK) for (var state : block.getStateDefinition().getPossibleStates())
            out("accept", ""+Block.getId(state), ""+BuddingAmethystBlock.canClusterGrowAtState(state));
        var neighbors = new ArrayList<BlockState>();
        for (var block : List.of(Blocks.AIR, Blocks.STONE, Blocks.WATER, Blocks.LAVA,
                Blocks.BUBBLE_COLUMN, Blocks.OAK_FENCE, Blocks.SMALL_AMETHYST_BUD,
                Blocks.MEDIUM_AMETHYST_BUD, Blocks.LARGE_AMETHYST_BUD, Blocks.AMETHYST_CLUSTER,
                Blocks.FIRE, Blocks.SHORT_GRASS)) {
            // Fence connection masks and fire age/attachment masks do not affect
            // this method; keep their two water states and default fire only.
            if (block == Blocks.OAK_FENCE) {
                neighbors.add(block.defaultBlockState());
                neighbors.add(block.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED,true));
            } else if (block == Blocks.FIRE) neighbors.add(block.defaultBlockState());
            else neighbors.addAll(block.getStateDefinition().getPossibleStates());
        }
        for (var origin : List.of(POS, new BlockPos(82,33,-24))) for (var neighbor : neighbors) for (int seed=0; seed<64; seed++) {
            prepare(Blocks.AIR.defaultBlockState(),Blocks.AIR.defaultBlockState());
            getChunkAt(origin).setBlockState(origin,Blocks.BUDDING_AMETHYST.defaultBlockState(),816);
            for (var side : Direction.values()) getChunkAt(origin.relative(side)).setBlockState(origin.relative(side),neighbor,816);
            writes.clear(); random.setSeed(seed);
            Blocks.BUDDING_AMETHYST.defaultBlockState().randomTick(this,origin,random);
            var states = new ArrayList<String>();
            for (var side : Direction.values()) states.add(""+Block.getId(getBlockState(origin.relative(side))));
            out("grow",origin.getX()+","+origin.getY()+","+origin.getZ(),""+Block.getId(neighbor),""+seed,
                String.join(",",states),String.join(";",writes),""+random.nextLong());
        }
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"amethyst-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        new AmethystVectors(server,storage).capture();
        System.exit(0);
    }
}

final class AmethystCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors);
        AmethystVectors.run();
    }
}
