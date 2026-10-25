import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Actual inherited FallingBlock onPlace/tick for every powder, including
 * blocked supports and the real FallingBlockEntity receiver captured at birth. */
final class PowderFallingVectors extends AnvilVectors {
    PowderFallingVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) {
        super(server, storage);
    }

    @Override void capture() {
        for (var block : BuiltInRegistries.BLOCK) {
            if (!(block instanceof ConcretePowderBlock)) continue;
            var state = block.defaultBlockState();
            for (var below : List.of(Blocks.AIR, Blocks.STONE, Blocks.WATER,
                    Blocks.FIRE, Blocks.COBWEB, Blocks.STONE_SLAB)) {
                for (boolean moved : new boolean[]{false, true}) {
                    prepare(state, below.defaultBlockState());
                    state.onPlace(this, POS, Blocks.AIR.defaultBlockState(), moved);
                    for (int tick = 0; tick <= 3; tick++) {
                        time = tick;
                        ticks.tick(tick, 65536, (pos, type) -> {
                            var current = getBlockState(pos);
                            if (current.is(type)) current.tick(this, pos, random);
                        });
                        var entity = added.isEmpty() ? null : added.getFirst();
                        out("tick", "" + Block.getId(state),
                            "" + Block.getId(below.defaultBlockState()), "" + moved,
                            "" + tick, "" + Block.getId(getBlockState(POS)),
                            "" + ticks.hasScheduledTick(POS, block), "" + added.size(),
                            "" + (entity == null ? -1 : Block.getId(entity.getBlockState())));
                    }
                }
            }
        }
    }

    static void run() throws Exception {
        var storage = LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),
            "powder-falling-vectors")).createAccess("world");
        var server = GameTestServer.create(Thread.currentThread(), storage,
            ServerPacksSource.createVanillaTrustedRepository(), Optional.empty(), false, 1);
        server.setPlayerList(new PlayerList(server, server.registries(),
            storage.createPlayerStorage(), new EmptyNotificationService()) {});
        new PowderFallingVectors(server, storage).capture();
        System.exit(0);
    }
}

final class PowderFallingCapture {
    public static void main(String[] args) throws Exception {
        var output = System.out;
        var errors = System.err;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(output);
        System.setErr(errors);
        PowderFallingVectors.run();
    }
}
