import java.util.*;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.monster.zombie.*;
import net.minecraft.world.level.pathfinder.PathType;
import net.minecraft.world.level.storage.*;
import static org.mockito.Mockito.*;

/** Real subclass constructors and NeutralMob's modern/legacy NBT read order. */
public final class ZombieSubclassVectors {
  static Object field(Object target,Class<?> owner,String name)throws Exception {
    var f=owner.getDeclaredField(name);f.setAccessible(true);return f.get(target);
  }
  static void capture(MonsterVectors.Fixture fixture)throws Exception {
    var drowned=new Drowned(EntityTypes.DROWNED,fixture);
    var piglin=new ZombifiedPiglin(EntityTypes.ZOMBIFIED_PIGLIN,fixture);
    System.out.println("monster-subclass\tdrowned\t"+drowned.getNavigation().getClass().getSimpleName()+"\t"+
      drowned.getMoveControl().getClass().getSimpleName()+"\t"+MonsterVectors.fbits(drowned.getPathfindingMalus(PathType.WATER))+"\t"+
      field(drowned,Drowned.class,"searchingForLand")+"\t"+MonsterVectors.fbits((Float)field(drowned,Drowned.class,"rangedAttackUncertainty")));
    System.out.println("monster-subclass\tzombified_piglin\t"+MonsterVectors.fbits(piglin.getPathfindingMalus(PathType.LAVA))+"\t"+
      field(piglin,ZombifiedPiglin.class,"playFirstAngerSoundIn")+"\t"+piglin.getPersistentAngerEndTime()+"\t"+
      field(piglin,ZombifiedPiglin.class,"ticksUntilNextAlert"));
    var level=mock(ServerLevel.class,RETURNS_DEEP_STUBS);
    when(level.registryAccess()).thenReturn(fixture.registryAccess());when(level.getDifficulty()).thenReturn(Difficulty.NORMAL);
    for(long time:new long[]{0,73,Long.MAX_VALUE})for(String input:List.of(
      "{}", "{AngerTime:40}", "{AngerTime:-10}", "{anger_end_time:0L,AngerTime:40}",
      "{anger_end_time:999L,angry_at:[I;1,2,3,4]}", "{anger_end_time:-1L,angry_at:[I;1,2,3,4]}",
      "{anger_end_time:'bad',AngerTime:5,angry_at:'bad'}", "{AngerTime:40,angry_at:[I;1,2,3,4]}")) {
      when(level.getGameTime()).thenReturn(time);
      var actor=new ZombifiedPiglin(EntityTypes.ZOMBIFIED_PIGLIN,level);
      var tag=TagParser.parseCompoundFully(input);
      actor.readPersistentAngerSaveData(level,TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      var output=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,level.registryAccess());
      actor.addPersistentAngerSaveData(output);var saved=output.buildResult();
      var again=new ZombifiedPiglin(EntityTypes.ZOMBIFIED_PIGLIN,level);
      again.readPersistentAngerSaveData(level,TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),saved));
      var repeat=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,level.registryAccess());
      again.addPersistentAngerSaveData(repeat);
      if(!saved.equals(repeat.buildResult()))throw new AssertionError(input);
      System.out.println("piglin-anger\t"+time+"\t"+input+"\t"+saved.getLongOr("anger_end_time",0)+"\t"+
        (saved.contains("angry_at")?saved.get("angry_at").toString():"-")+"\t"+(actor.getTargetUnchecked()==null));
    }
  }
}
