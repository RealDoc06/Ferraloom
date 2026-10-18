import java.lang.reflect.Field;
import java.util.*;
import com.mojang.serialization.Dynamic;
import com.mojang.serialization.Lifecycle;
import net.minecraft.nbt.*;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.dedicated.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.*;
import net.minecraft.world.level.storage.PrimaryLevelData;

/** Actual LevelSettings codec and server difficulty mutation without IO threads. */
public final class DifficultyVectors {
  static void set(Object receiver,Class<?> owner,String field,Object value)throws Exception {
    var f=owner.getDeclaredField(field);f.setAccessible(true);f.set(receiver,value);
  }
  public static void run()throws Exception {
    String[] records={"{}","{difficulty_settings:{}}",
      "{difficulty_settings:{difficulty:hard,hardcore:1b,locked:1b}}",
      "{difficulty_settings:{difficulty:easy,hardcore:0b,locked:0b}}",
      "{difficulty_settings:{difficulty:peaceful,hardcore:0b,locked:1b}}",
      "{difficulty_settings:{difficulty:normal,hardcore:1b,locked:0b}}",
      "{difficulty_settings:{difficulty:wrong,hardcore:1b,locked:1b}}",
      "{difficulty_settings:{difficulty:hard,hardcore:1b}}",
      "{difficulty_settings:{difficulty:hard,locked:1b}}",
      "{difficulty_settings:{difficulty:hard,hardcore:yes,locked:1b}}",
      "{difficulty_settings:{difficulty:hard,hardcore:1b,locked:yes}}",
      "{difficulty_settings:{difficulty:3,hardcore:1b,locked:1b}}",
      "{difficulty_settings:{difficulty:hard,hardcore:256,locked:-1b}}",
      "{difficulty_settings:{difficulty:hard,hardcore:1.5d,locked:0.0d}}",
      "{difficulty_settings:{difficulty:hard,hardcore:0.5d,locked:-0.5d}}",
      "{difficulty_settings:{difficulty:hard,hardcore:0L,locked:4294967296L}}"};
    for(String record:records) {
      var settings=LevelSettings.parse(new Dynamic<>(NbtOps.INSTANCE,TagParser.parseCompoundFully(record)),WorldDataConfiguration.DEFAULT).difficultySettings();
      System.out.println("difficulty-read\t"+record+"\t"+settings.difficulty().getSerializedName()+"\t"+settings.hardcore()+"\t"+settings.locked());
    }
    // Constructor bypass only wires services not used by setDifficulty; the
    // mutation itself is the real MinecraftServer method on real WorldData.
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);
    var allocator=(sun.misc.Unsafe)field.get(null);
    var server=(DedicatedServer)allocator.allocateInstance(DedicatedServer.class);
    var players=(DedicatedPlayerList)allocator.allocateInstance(DedicatedPlayerList.class);
    set(players,PlayerList.class,"players",List.of());
    set(server,MinecraftServer.class,"playerList",players);
    set(server,MinecraftServer.class,"levels",Map.of());
    for(boolean hardcore:List.of(false,true))for(boolean locked:List.of(false,true))for(Difficulty requested:Difficulty.values()) {
      var settings=new LevelSettings("fixture",GameType.SURVIVAL,new LevelSettings.DifficultySettings(Difficulty.NORMAL,hardcore,locked),true,WorldDataConfiguration.DEFAULT);
      var world=new PrimaryLevelData(settings,PrimaryLevelData.SpecialWorldProperty.NONE,Lifecycle.stable());
      set(server,MinecraftServer.class,"worldData",world);
      server.setDifficulty(requested,true);
      System.out.println("difficulty-initial\t"+hardcore+"\t"+locked+"\t"+requested.getSerializedName()+"\t"+world.getDifficulty().getSerializedName()+"\t"+world.isDifficultyLocked());
    }
  }
}
