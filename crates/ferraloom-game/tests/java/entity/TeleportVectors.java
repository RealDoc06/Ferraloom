import java.util.*;
/** Native independent player build ability and game-mode application. */
public final class TeleportVectors {
  public static void run(net.minecraft.world.level.Level fixture) throws Exception {
    for(var type:net.minecraft.world.level.GameType.values()) for(boolean flying:List.of(false,true)) {
      var player=new LivingVectors(fixture);player.getAbilities().flying=flying;
      type.updatePlayerAbilities(player.getAbilities());
      boolean mayBuild=player.mayBuild();boolean resultFlying=player.getAbilities().flying;
      player.getAbilities().mayBuild=!mayBuild;
      var saved=net.minecraft.world.entity.player.Abilities.Packed.CODEC.encodeStart(net.minecraft.nbt.NbtOps.INSTANCE,player.getAbilities().pack()).getOrThrow().asCompound().orElseThrow();
      System.out.println("player-may-build\t"+type.getId()+"\t"+flying+"\t"+mayBuild+","+resultFlying+","+player.mayBuild()+","+saved.getBooleanOr("mayBuild",true));
    }
  }
}
