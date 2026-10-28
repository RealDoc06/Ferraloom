import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.syncher.*;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.ai.goal.GoalSelector;
import net.minecraft.world.entity.ai.control.*;
import net.minecraft.world.entity.ai.navigation.PathNavigation;
import net.minecraft.world.entity.ai.sensing.Sensing;
import net.minecraft.world.level.*;
import net.minecraft.world.level.pathfinder.PathType;
import static org.mockito.Mockito.*;

/** Actual constructor phases, Mob enum maluses and final serverAiStep ordering. */
public final class MobFoundationVectors {
 static long seed;
 static final class Receiver extends Cow {
  static final EntityDataAccessor<Integer> MARKER=SynchedEntityData.defineId(Receiver.class,EntityDataSerializers.INT);
  boolean inherit;Entity controller;List<String> events;
  Receiver(ServerLevel level){super(EntityTypes.COW,level);}
  @Override protected void defineSynchedData(SynchedEntityData.Builder data){super.defineSynchedData(data);this.random.setSeed(seed);data.define(MARKER,this.random.nextInt(13));}
  @Override protected boolean shouldPassengersInheritMalus(){return inherit;}
  @Override public LivingEntity getControllingPassenger(){return (LivingEntity)controller;}
  @Override protected void customServerAiStep(ServerLevel level){events.add("custom");}
  void runAi(){super.serverAiStep();}
  boolean deadFlag(){return dead;}
 }
 static void field(Object target,Class<?> owner,String name,Object value)throws Exception{var f=owner.getDeclaredField(name);f.setAccessible(true);f.set(target,value);}
 static String f(float value){return Integer.toHexString(Float.floatToIntBits(value));}
 public static void main(String[] args)throws Exception {
  var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
  var registries=NeighborVectors.load();var level=mock(ServerLevel.class,RETURNS_DEEP_STUBS);when(level.registryAccess()).thenReturn(registries);when(level.getDifficulty()).thenReturn(Difficulty.NORMAL);
  for(long value:new long[]{0,1,2,7,98,1234,-1,Long.MIN_VALUE}) {
   seed=value;var actor=new Receiver(level);
   System.out.println("mob-constructor\t"+value+"\t"+actor.getEntityData().get(Receiver.MARKER)+"\t"+f(actor.getYRot())+"\t"+f(actor.getHealth())+"\t"+Long.toHexString(actor.getRandom().nextLong()));
  }
  for(var type:PathType.values())for(boolean own:List.of(false,true))for(boolean mounted:List.of(false,true))for(boolean controlled:List.of(false,true))for(boolean inherit:List.of(false,true)) {
   var actor=new Receiver(level);var vehicle=new Receiver(level);vehicle.inherit=inherit;vehicle.controller=controlled?actor:null;
   if(own)actor.setPathfindingMalus(type,-2.25f);vehicle.setPathfindingMalus(type,7.75f);
   if(mounted)field(actor,Entity.class,"vehicle",vehicle);
   float before=actor.getPathfindingMalus(type);actor.setPathfindingMalus(type,1.5f);
   System.out.println("mob-malus\t"+type.ordinal()+"\t"+own+"\t"+mounted+"\t"+controlled+"\t"+inherit+"\t"+f(before)+"\t"+f(actor.getPathfindingMalus(type))+"\t"+f(vehicle.getPathfindingMalus(type)));
  }
  for(var difficulty:Difficulty.values())for(boolean hasTarget:List.of(false,true))for(float health:new float[]{0,1,5,6.6f,10,19.999f,20,Float.NaN,Float.POSITIVE_INFINITY}) {
   when(level.getDifficulty()).thenReturn(difficulty);var actor=new Receiver(level);if(hasTarget)actor.setTarget(new Receiver(level));actor.setHealth(health);
   System.out.println("mob-fall\t"+difficulty.getId()+"\t"+hasTarget+"\t"+f(health)+"\t"+actor.getMaxFallDistance());
  }
  for(var difficulty:Difficulty.values())for(String mode:List.of("survival","creative","spectator"))for(boolean invulnerable:List.of(false,true))for(boolean alive:List.of(false,true)) {
   when(level.getDifficulty()).thenReturn(difficulty);var actor=new Receiver(level);
   var target=mock(net.minecraft.world.entity.player.Player.class,CALLS_REAL_METHODS);
   doReturn(EntityTypes.PLAYER.builtInRegistryHolder()).when(target).typeHolder();
   var abilities=new net.minecraft.world.entity.player.Abilities();abilities.invulnerable=invulnerable;
   doReturn(abilities).when(target).getAbilities();doReturn(mode.equals("creative")).when(target).isCreative();doReturn(mode.equals("spectator")).when(target).isSpectator();
   doReturn(alive).when(target).isAlive();doReturn(false).when(target).isPermanentlyInvulnerable();doReturn(false).when(target).isTemporarilyInvulnerable();
   actor.setTarget(target);boolean stored=actor.getTargetUnchecked()==target;
   field(actor,Mob.class,"target",target);
   System.out.println("mob-target\t"+difficulty.getId()+"\t"+mode+"\t"+invulnerable+"\t"+alive+"\t"+stored+"\t"+(actor.getTarget()==target)+"\t"+(actor.getTargetUnchecked()==target)+"\t"+actor.canAttack(target));
  }
  for(int tick:new int[]{Integer.MIN_VALUE,-1,0,1,2,3,4,Integer.MAX_VALUE})for(int id:new int[]{1,2,Integer.MIN_VALUE,Integer.MAX_VALUE})for(boolean sensingChange:List.of(false,true))for(boolean targetChange:List.of(false,true)) {
   var actor=new Receiver(level);var events=new ArrayList<String>();actor.events=events;actor.tickCount=tick;actor.setId(id);actor.setNoActionTime(Integer.MAX_VALUE);
   var sensing=mock(Sensing.class);doAnswer(call->{events.add("sensing:"+actor.getNoActionTime());if(sensingChange){actor.tickCount=3;actor.setId(2);}return null;}).when(sensing).tick();field(actor,Mob.class,"sensing",sensing);
   for(boolean target:List.of(true,false)) {
    var selector=mock(GoalSelector.class);String label=target?"target":"goal";
    doAnswer(call->{events.add(label+":full");if(target&&targetChange)actor.tickCount=0;return null;}).when(selector).tick();
    doAnswer(call->{events.add(label+":running:"+call.getArgument(0));if(target&&targetChange)actor.tickCount=0;return null;}).when(selector).tickRunningGoals(anyBoolean());
    field(actor,Mob.class,target?"targetSelector":"goalSelector",selector);
   }
   var navigation=mock(PathNavigation.class);doAnswer(call->{events.add("navigation");return null;}).when(navigation).tick();field(actor,Mob.class,"navigation",navigation);
   var move=mock(MoveControl.class);doAnswer(call->{events.add("move");return null;}).when(move).tick();field(actor,Mob.class,"moveControl",move);
   var look=mock(LookControl.class);doAnswer(call->{events.add("look");return null;}).when(look).tick();field(actor,Mob.class,"lookControl",look);
   var jump=mock(JumpControl.class);doAnswer(call->{events.add("jump");return null;}).when(jump).tick();field(actor,Mob.class,"jumpControl",jump);
   actor.runAi();
   System.out.println("mob-tick\t"+tick+"\t"+id+"\t"+sensingChange+"\t"+targetChange+"\t"+String.join(";",events));
  }
 }
}
