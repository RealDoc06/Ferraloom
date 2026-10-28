import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.util.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Actual BaseSpawner server ticks, including EntityGetter's native player query. */
public final class SpawnerTickVectors {
  static final BlockPos POS = new BlockPos(8,64,8);
  static final class SpawnAttempt extends RuntimeException {}
  public static void main(String[] args) throws Exception {
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
    var world=NeighborVectors.load();
    var builtin=RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),world.listRegistries()));
    var inputs=List.of(
      "{}",
      "{Delay:-1s,MinSpawnDelay:4,MaxSpawnDelay:9,SpawnCount:0,SpawnPotentials:[{data:{entity:{id:'pig'}},weight:2},{data:{entity:{id:'cow'}},weight:5}]}",
      "{Delay:-1s,MinSpawnDelay:4,MaxSpawnDelay:4,SpawnCount:0,SpawnPotentials:[]}",
      "{Delay:-1s,MinSpawnDelay:0,MaxSpawnDelay:0,SpawnCount:0,SpawnPotentials:[]}",
      "{Delay:0s,SpawnCount:0,SpawnPotentials:[{data:{entity:{id:'cow'}},weight:1}]}",
      "{Delay:-7s,SpawnCount:-1,SpawnPotentials:[]}",
      "{Delay:0s,SpawnCount:2,SpawnData:{entity:{id:'pig'}},SpawnPotentials:[]}",
      "{Delay:0s,SpawnCount:2,SpawnPotentials:[{data:{entity:{id:'cow'}},weight:1}]}",
      "{RequiredPlayerRange:0}",
      "{RequiredPlayerRange:-1}"
    );
    for(var input:inputs)for(var mode:List.of("none","survival","boundary","inside","far","creative","spectator","dead","removed","disabled"))for(long seed:new long[]{0,19})for(int steps:new int[]{1,3,6}) {
      var receiver=new SpawnerBlockEntity(POS,Blocks.SPAWNER.defaultBlockState());
      receiver.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,context,TagParser.parseCompoundFully(input)));
      var random=RandomSource.create(seed);var level=mock(ServerLevel.class);
      FactoryBatchTwoVectors.set(level,"random",random);doReturn(random).when(level).getRandom();
      doReturn(!mode.equals("disabled")).when(level).isSpawnerBlockEnabled();
      doReturn(Blocks.SPAWNER.defaultBlockState()).when(level).getBlockState(POS);
      var notifications=new ArrayList<String>();var events=new ArrayList<String>();
      doAnswer(c->{notifications.add(Block.getId(c.getArgument(1))+","+Block.getId(c.getArgument(2))+","+c.getArgument(3));return null;}).when(level).sendBlockUpdated(eq(POS),any(),any(),anyInt());
      doAnswer(c->{events.add(c.getArgument(2)+":"+c.getArgument(3));return null;}).when(level).blockEvent(any(),any(),anyInt(),anyInt());
      var players=new ArrayList<ServerPlayer>();
      if(!mode.equals("none")) {
        var player=mock(ServerPlayer.class);players.add(player);
        double distance=switch(mode){case "boundary"->16;case "inside"->15.999;case "far"->1000;default->0;};
        doReturn(8.5+distance).when(player).getX();doReturn(64.5).when(player).getY();doReturn(8.5).when(player).getZ();
        doReturn(mode.equals("spectator")).when(player).isSpectator();
        doReturn(!mode.equals("dead")&&!mode.equals("removed")).when(player).isAlive();
        doCallRealMethod().when(player).distanceToSqr(anyDouble(),anyDouble(),anyDouble());
      }
      doReturn(players).when(level).players();
      doCallRealMethod().when(level).hasNearbyAlivePlayer(anyDouble(),anyDouble(),anyDouble(),anyDouble());
      // The unported service starts before type/Pos decoding and position RNG.
      doThrow(new SpawnAttempt()).when(level).registryAccess();
      receiver.setLevel(level);
      var ticks=new ArrayList<String>();
      for(int step=0;step<steps;step++) {
        notifications.clear();events.clear();String result="ok";
        try {SpawnerBlockEntity.serverTick(level,POS,Blocks.SPAWNER.defaultBlockState(),receiver);}catch(SpawnAttempt boundary){result="spawn";}
        ticks.add(result+"|"+receiver.saveWithFullMetadata(context)+"|"+String.join(";",notifications)+"|"+String.join(";",events));
        if(result.equals("spawn"))break;
      }
      System.out.println("spawner-tick\t"+input+"\t"+mode+"\t"+seed+"\t"+steps+"\t"+String.join("\t",ticks)+"\t"+random.nextLong());
    }
  }
}
