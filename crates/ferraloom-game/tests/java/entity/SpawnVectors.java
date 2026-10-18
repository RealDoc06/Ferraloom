import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.phys.Vec2;
import net.minecraft.world.phys.Vec3;

/** PrepareSpawnTask's real saved-position codec and final Entity.snapTo. */
public final class SpawnVectors {
  static String angles(float... values) {
    var bits = new ArrayList<String>();
    for (float value : values) bits.add(Integer.toHexString(Float.floatToRawIntBits(value)));
    return String.join(",", bits);
  }

  public static void run(Level level) throws Exception {
    String[][] cases = {
      {"fresh_zero", "-", "0", "0"},
      {"fresh_respawn", "-", "30", "-12.5"},
      {"saved_without_rotation", "{}", "30", "-12.5"},
      {"saved_rotation", "{Rotation:[37.25f,-28.125f]}", "30", "-12.5"},
      {"saved_unwrapped", "{Rotation:[721.25f,120.25f]}", "30", "-12.5"},
      {"saved_signed_zero", "{Rotation:[-0.0f,-0.0f]}", "30", "-12.5"},
      {"rotation_long", "{Rotation:[70.0f,10.0f,99.0f]}", "30", "-12.5"},
      {"rotation_short", "{Rotation:[70.0f]}", "30", "-12.5"},
      {"rotation_array", "{Rotation:[I;45,-15]}", "30", "-12.5"},
      {"rotation_partial", "{Rotation:[70.0f,10.0f,'invalid']}", "30", "-12.5"},
      {"invalid_dimension", "{Dimension:'Bad Dimension',Rotation:[70.0f,10.0f]}", "30", "-12.5"},
      {"position_short", "{Pos:[1.0d,2.0d],Rotation:[70.0f,10.0f]}", "30", "-12.5"},
      {"position_long", "{Pos:[1.0d,2.0d,3.0d,4.0d],Rotation:[70.0f,10.0f]}", "30", "-12.5"}
    };
    var spawn = new Vec3(8.5, 64.25, -12.75);
    for (String[] scenario : cases) {
      var tag = scenario[1].equals("-") ? null : TagParser.parseCompoundFully(scenario[1]);
      var input = tag == null ? null : TagValueInput.create(ProblemReporter.DISCARDING, level.registryAccess(), tag);
      var saved = input == null ? ServerPlayer.SavedPosition.EMPTY : input.read(ServerPlayer.SavedPosition.MAP_CODEC).orElse(ServerPlayer.SavedPosition.EMPTY);
      var angle = saved.rotation().orElse(new Vec2(Float.parseFloat(scenario[2]), Float.parseFloat(scenario[3])));
      var player = new LivingVectors(level);
      player.setYRot(4.214188f);
      player.setXRot(5.0f);
      if (input != null) player.load(input);
      // Isolate the final snap from Player.load's separate enchantment-seed draw.
      player.getRandom().setSeed(12345);
      player.snapTo(spawn, angle.x, angle.y);
      System.out.println("player-spawn-rotation\t"+scenario[0]+"\t"+scenario[1]+"\t"+scenario[2]+","+scenario[3]+"\t"+saved.rotation().isPresent()+"\t"+angles(angle.x,angle.y)+"\t"+angles(player.getYRot(),player.getXRot(),player.yRotO,player.xRotO)+"\t"+TrackingVectors.vec(player.position())+"\t"+TrackingVectors.vec(new Vec3(player.xo,player.yo,player.zo))+"\t"+TrackingVectors.vec(player.oldPosition())+"\t"+Long.toHexString(player.getRandom().nextLong()));
    }
  }
}
