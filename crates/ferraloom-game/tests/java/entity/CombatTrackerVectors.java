import java.lang.reflect.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.BlockPos;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;

/** Actual CombatTracker state transitions and reentrant parent callbacks. */
public final class CombatTrackerVectors {
  static final class Receiver extends Player {
    List<String> events=new ArrayList<>();
    boolean reentrant;
    Receiver(Level level){super(level,new GameProfile(new UUID(7,8),"combat"));}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public void onEnterCombat(){event("enter");if(reentrant){getCombatTracker().recordDamage(level().damageSources().generic(),2.5f);event("entered");}}
    @Override public void onLeaveCombat(){event("leave");if(reentrant){getCombatTracker().recordDamage(level().damageSources().generic(),3.5f);event("left");}}
    void event(String name){try{events.add(name+":"+state(this));}catch(Exception error){throw new RuntimeException(error);}}
  }
  static String entries(CombatTracker tracker)throws Exception {
    var values=(List<CombatEntry>)ExperienceVectors.field(CombatTracker.class,"entries").get(tracker);
    var text=new ArrayList<String>();
    for(var entry:values)text.add((entry.source().getEntity()==null?"env":"living")+","+ExperienceVectors.f(entry.damage())+","+(entry.fallLocation()==null?"-":entry.fallLocation().id())+","+ExperienceVectors.f(entry.fallDistance()));
    return text.isEmpty()?"-":String.join("/",text);
  }
  static String state(Receiver player)throws Exception {
    var tracker=player.getCombatTracker();
    return ExperienceVectors.field(CombatTracker.class,"takingDamage").getBoolean(tracker)+","+ExperienceVectors.field(CombatTracker.class,"inCombat").getBoolean(tracker)+","+ExperienceVectors.field(CombatTracker.class,"lastDamageTime").getInt(tracker)+","+ExperienceVectors.field(CombatTracker.class,"combatStartTime").getInt(tracker)+","+ExperienceVectors.field(CombatTracker.class,"combatEndTime").getInt(tracker)+","+tracker.getCombatDuration()+";"+entries(tracker);
  }
  public static void run(Level level)throws Exception {
    for(boolean reentrant:new boolean[]{false,true}){
      var player=new Receiver(level);player.reentrant=reentrant;var attacker=new LivingVectors(level);
      String[][] steps={{"check","0","20"},{"env","1","20"},{"check","101","20"},{"check","102","20"},{"living","103","20"},{"env","104","20"},{"check","404","20"},{"check","405","20"},{"living","406","20"},{"check","407","0"},{"living","408","0"},{"check","409","NaN"},{"living","2147483643","20"},{"check","-2147483352","20"},{"check","-2147483351","20"}};
      for(int i=0;i<steps.length;i++){
        var step=steps[i];player.tickCount=Integer.parseInt(step[1]);player.setHealth(Float.parseFloat(step[2]));player.fallDistance=5.25;player.events.clear();
        if(step[0].equals("check"))player.getCombatTracker().recheckStatus();
        else player.getCombatTracker().recordDamage(step[0].equals("env")?level.damageSources().generic():level.damageSources().mobAttack(attacker),4);
        System.out.println("combat-tracker\t"+reentrant+"\t"+i+"\t"+String.join("\t",step)+"\t"+state(player)+"\t"+(player.events.isEmpty()?"-":String.join("|",player.events)));
      }
    }
    var pos=new BlockPos(8,64,8);
    var blocks=List.of(Blocks.LADDER,Blocks.OAK_TRAPDOOR,Blocks.VINE,Blocks.WEEPING_VINES,Blocks.WEEPING_VINES_PLANT,Blocks.TWISTING_VINES,Blocks.TWISTING_VINES_PLANT,Blocks.SCAFFOLDING,Blocks.STONE);
    for(boolean water:new boolean[]{false,true})for(int i=-1;i<blocks.size();i++){
      var player=new Receiver(level);player.setPos(8.5,64,8.5);player.fallDistance=Double.MAX_VALUE;
      ExperienceVectors.field(Entity.class,"wasTouchingWater").setBoolean(player,water);
      if(i>=0){ClimbVectors.put(level,pos,blocks.get(i).defaultBlockState());ExperienceVectors.field(LivingEntity.class,"lastClimbablePos").set(player,Optional.of(pos));}
      player.getCombatTracker().recordDamage(level.damageSources().fall(),1);
      System.out.println("combat-fall-location\t"+water+"\t"+(i<0?-1:Block.getId(blocks.get(i).defaultBlockState()))+"\t"+entries(player.getCombatTracker()));
    }
  }
}
