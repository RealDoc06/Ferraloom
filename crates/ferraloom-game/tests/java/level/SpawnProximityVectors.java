import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.*;
import net.minecraft.world.level.storage.LevelStorageSource;
import com.mojang.authlib.GameProfile;

/** Actual DistanceManager graph and ChunkMap player-distance predicate. */
public final class SpawnProximityVectors {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var lines=new ArrayList<String>();
        var manager=new LoadingVectors.Manager(new TicketStorage());
        // addPlayer only stores this opaque token; hasPlayersNearby never dereferences it.
        manager.addPlayer(SectionPos.of(0,0,0),null);
        for(var pos:List.of(new ChunkPos(0,0),new ChunkPos(5,5),new ChunkPos(6,0),new ChunkPos(8,0),new ChunkPos(9,0)))
            lines.add("counter\t"+pos.x()+"\t"+pos.z()+"\t"+manager.hasPlayersNearby(pos.pack()));
        manager.removePlayer(SectionPos.of(0,0,0),null);
        lines.add("removed\t0\t0\t"+manager.hasPlayersNearby(new ChunkPos(0,0).pack()));
        var temp=Files.createTempDirectory("ferraloom-spawn-proximity-");
        try {
            var access=LevelStorageSource.createDefault(temp).createAccess("world");
            var packs=ServerPacksSource.createPackRepository(access);
            try(var server=new PublishedLightTrace.Server(access,packs,PublishedLightTrace.load(packs))) {
                var level=server.getLevel(Level.OVERWORLD);
                var player=new ServerPlayer(server,level,new GameProfile(new UUID(1,2),"spawn"),ClientInformation.createDefault());
                var predicate=ChunkMap.class.getDeclaredMethod("playerIsCloseEnoughForSpawning",ServerPlayer.class,ChunkPos.class);
                predicate.setAccessible(true);
                var mode=ServerPlayerGameMode.class.getDeclaredField("gameModeForPlayer");mode.setAccessible(true);
                for(double[] row:new double[][]{{8,0,8,64,8,0},{8,0,8.0001,64,8,0},{8,0,7.9999,64,8,0},{6,6,8,64,8,0},{6,5,8,10000,8,0},{6,5,8,64,8,1}}) {
                    player.setPos(row[2],row[3],row[4]);
                    mode.set(player.gameMode,row[5]==0?GameType.SURVIVAL:GameType.SPECTATOR);
                    boolean result=(boolean)predicate.invoke(level.getChunkSource().chunkMap,player,new ChunkPos((int)row[0],(int)row[1]));
                    lines.add("player\t"+(int)row[0]+"\t"+(int)row[1]+"\t"+row[2]+"\t"+row[3]+"\t"+row[4]+"\t"+(row[5]!=0)+"\t"+result);
                }
            }
        } finally {
            try(var files=Files.walk(temp)){for(var path:files.sorted(Comparator.reverseOrder()).toList())Files.delete(path);}
            net.minecraft.util.Util.shutdownExecutors();
        }
        Files.write(Path.of(args[0]),lines);
    }
}
