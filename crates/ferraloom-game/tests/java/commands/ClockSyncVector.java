import com.google.gson.*;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundSetTimePacket;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.players.PlayerList;
import static org.mockito.Mockito.*;

/** Captures the real server's periodic sync, including its encoded empty map. */
public final class ClockSyncVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var vectors = new JsonArray();
        for (long time : new long[]{0,20,-1,Long.MIN_VALUE,Long.MAX_VALUE}) {
            var server = mock(MinecraftServer.class);
            var level = mock(ServerLevel.class);
            var players = mock(PlayerList.class);
            var field = MinecraftServer.class.getDeclaredField("playerList");
            field.setAccessible(true); field.set(server, players);
            doReturn(level).when(server).overworld();
            doReturn(time).when(level).getGameTime();
            doCallRealMethod().when(server).forceGameTimeSynchronization();
            doAnswer(invocation -> {
                ClientboundSetTimePacket packet = invocation.getArgument(0);
                var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(), RegistryAccess.EMPTY);
                try {
                    ClientboundSetTimePacket.STREAM_CODEC.encode(buffer, packet);
                    var row = new JsonObject();
                    row.addProperty("game_time", packet.gameTime());
                    row.addProperty("clock_count", packet.clockUpdates().size());
                    row.addProperty("bytes", ByteBufUtil.hexDump(buffer));
                    vectors.add(row);
                } finally { buffer.release(); }
                return null;
            }).when(players).broadcastAll(any());
            server.forceGameTimeSynchronization();
            verify(players).broadcastAll(any());
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
