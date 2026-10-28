import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.gameevent.GameEvent;
import static org.mockito.Mockito.*;

/** Actual Animal/Player XP reads and the native death callback ordering. */
public final class DeathSeamVectors {
 static final class PendingLoot extends RuntimeException {}
 static final class Receiver extends Cow {
  final List<String> events=new ArrayList<>();boolean consume;
  Receiver(ServerLevel level){super(EntityTypes.COW,level);}
  @Override protected void handleKillingBlow(){events.add("killing:"+dead);super.handleKillingBlow();}
  @Override public void gameEvent(Holder<GameEvent> event){
   events.add("event:"+event.unwrapKey().orElseThrow().identifier()+":"+dead+":"+wasExperienceConsumed()+":"+getUseItem().isEmpty()+":"+getUseItemRemainingTicks());
   if(consume&&event.is(GameEvent.ENTITY_DIE))skipDropExperience();
  }
  @Override protected void dropAllDeathLoot(ServerLevel level,DamageSource source){events.add("loot:"+dead+":"+wasExperienceConsumed());throw new PendingLoot();}
 }
 public static void main(String[] args)throws Exception {
  var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
  var registries=NeighborVectors.load();var rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);
  var level=mock(ServerLevel.class,RETURNS_DEEP_STUBS);when(level.registryAccess()).thenReturn(registries);when(level.getGameRules()).thenReturn(rules);when(level.damageSources()).thenReturn(new DamageSources(registries));
  for(long seed:new long[]{0,1,2,7,98,1234,-1,Long.MIN_VALUE})for(boolean baby:List.of(false,true))for(boolean consumed:List.of(false,true)) {
   var cow=new Cow(EntityTypes.COW,level);cow.setBaby(baby);cow.getRandom().setSeed(seed);if(consumed)cow.skipDropExperience();
   int reward=cow.getExperienceReward(level,null);
   System.out.println("death-xp-cow\t"+seed+"\t"+baby+"\t"+consumed+"\t"+cow.shouldDropExperience()+"\t"+reward+"\t"+Long.toHexString(cow.getRandom().nextLong())+"\t"+cow.wasExperienceConsumed());
  }
  for(boolean keep:List.of(false,true))for(boolean spectator:List.of(false,true))for(int xp:new int[]{-306783377,-1,0,1,14,15,100,Integer.MAX_VALUE})for(String killerKind:List.of("none","living","nonliving")) {
   rules.set(GameRules.KEEP_INVENTORY,keep,null);
   var player=mock(Player.class,CALLS_REAL_METHODS);when(player.isSpectator()).thenReturn(spectator);player.experienceLevel=xp;
   Entity killer=switch(killerKind){case "living"->new Cow(EntityTypes.COW,level);case "nonliving"->new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,level);default->null;};
   System.out.println("death-xp-player\t"+keep+"\t"+spectator+"\t"+xp+"\t"+killerKind+"\t"+player.getExperienceReward(level,killer));
  }
  for(boolean removed:List.of(false,true))for(boolean consumed:List.of(false,true))for(boolean consume:List.of(false,true)) {
   var cow=new Receiver(level);cow.consume=consume;if(removed)cow.discard();if(consumed)cow.skipDropExperience();
   String result="ok";try{cow.die(level.damageSources().generic());}catch(PendingLoot expected){result="pending-loot";}
   cow.die(level.damageSources().generic());
   System.out.println("death-prefix\t"+removed+"\t"+consumed+"\t"+consume+"\t"+result+"\t"+String.join(";",cow.events));
  }
 }
}
