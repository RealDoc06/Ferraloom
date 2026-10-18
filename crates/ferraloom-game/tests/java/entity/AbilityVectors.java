import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.*;
import net.minecraft.world.level.storage.TagValueInput;

/** Native ability speed reads, Player virtual flying speed, and loaded abilities
 * followed by the ServerPlayer game-mode application step. */
public final class AbilityVectors {
  public static void run(Level level)throws Exception {
    for(float speed:new float[]{0.05f,-0.25f,Float.NaN,Float.POSITIVE_INFINITY})for(boolean flying:new boolean[]{false,true})for(boolean passenger:new boolean[]{false,true})for(boolean sprint:new boolean[]{false,true}){
      var player=new TravelVectors(level);player.getAbilities().setFlyingSpeed(speed);player.getAbilities().flying=flying;player.sprint(sprint);
      ExperienceVectors.field(Entity.class,"vehicle").set(player,passenger?new PrimedTnt(EntityTypes.TNT,level):null);
      System.out.println("player-flying-speed\t"+TravelVectors.f(speed)+"\t"+flying+"\t"+passenger+"\t"+sprint+"\t"+TravelVectors.f(player.flyingSpeed()));
    }
    String[] inputs={"{}","{abilities:{}}","{abilities:{flying:1b,flySpeed:0.125f,walkSpeed:-0.25f}}",
      "{abilities:{flying:'bad',flySpeed:'bad',walkSpeed:'bad'}}","{abilities:{flying:256,flySpeed:0.15d,walkSpeed:2}}"};
    for(GameType mode:GameType.values())for(int i=0;i<inputs.length;i++){
      var input=TagParser.parseCompoundFully(inputs[i]);input.putInt("playerGameType",mode.getId());
      var player=new TravelVectors(level);player.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),input));
      mode.updatePlayerAbilities(player.getAbilities());
      System.out.println("player-ability-speeds\t"+mode.getId()+"\t"+i+"\t"+input+"\t"+TravelVectors.f(player.getAbilities().getFlyingSpeed())+"\t"+TravelVectors.f(player.getAbilities().getWalkingSpeed())+"\t"+player.getAbilities().flying);
    }
  }
}
