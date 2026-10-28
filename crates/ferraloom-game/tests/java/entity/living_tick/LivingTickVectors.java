import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Native inherited aiStep: thresholds, NoAI, immobility and callback order. */
public final class LivingTickVectors {
 static String v(Vec3 value){return Long.toHexString(Double.doubleToRawLongBits(value.x))+","+Long.toHexString(Double.doubleToRawLongBits(value.y))+","+Long.toHexString(Double.doubleToRawLongBits(value.z));}
 static String f(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
 static class Receiver extends Cow {
  boolean simulate=true; final List<String> events=new ArrayList<>();
  Receiver(ServerLevel level){super(EntityTypes.COW,level);}
  @Override public boolean isClientAuthoritative(){return !simulate;}
  @Override public boolean canFreeze(){return false;}
  @Override protected net.minecraft.world.level.block.state.BlockState getBlockStateOnLegacy(){return Blocks.STONE.defaultBlockState();}
  @Override protected void customServerAiStep(ServerLevel level){}
  @Override public void travel(Vec3 input){events.add("travel:"+v(input)+":"+v(getDeltaMovement()));}
  @Override protected void applyEffectsFromBlocks(){events.add("blocks");}
  @Override protected void pushEntities(){events.add("push");}
  void inputs(){xxa=0.75f;yya=-0.125f;zza=-0.3f;setJumping(false);}
  String inputsOut(){return f(xxa)+","+f(yya)+","+f(zza);}
 }
 static void field(Object target,String name,Object value)throws Exception{var f=Mob.class.getDeclaredField(name);f.setAccessible(true);f.set(target,value);}
 public static void main(String[] args)throws Exception {
  var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
  var registries=NeighborVectors.load();var level=mock(ServerLevel.class,RETURNS_DEEP_STUBS);when(level.registryAccess()).thenReturn(registries);
  Vec3[] velocities={new Vec3(.002,.0029,.002),new Vec3(.003,.003,0),new Vec3(-0.,-0.,-0.),new Vec3(Math.nextDown(.003),Math.nextDown(-.003),0),new Vec3(.0029,-.0031,.0029),new Vec3(Double.NaN,Double.POSITIVE_INFINITY,Double.NEGATIVE_INFINITY)};
  int index=0;
  for(Vec3 velocity:velocities)for(boolean noAi:List.of(false,true))for(boolean dead:List.of(false,true))for(boolean simulate:List.of(false,true)) {
   var actor=new Receiver(level);actor.setId(1);actor.setPos(8.5,64,8.5);actor.setNoAi(noAi);actor.setHealth(dead?0:10);actor.simulate=simulate;actor.inputs();actor.setDeltaMovement(velocity);
   var sensing=mock(net.minecraft.world.entity.ai.sensing.Sensing.class);doAnswer(call->{actor.events.add("ai");return null;}).when(sensing).tick();field(actor,"sensing",sensing);
   field(actor,"targetSelector",mock(net.minecraft.world.entity.ai.goal.GoalSelector.class));field(actor,"goalSelector",mock(net.minecraft.world.entity.ai.goal.GoalSelector.class));
   field(actor,"navigation",mock(net.minecraft.world.entity.ai.navigation.PathNavigation.class));field(actor,"moveControl",mock(net.minecraft.world.entity.ai.control.MoveControl.class));field(actor,"lookControl",mock(net.minecraft.world.entity.ai.control.LookControl.class));field(actor,"jumpControl",mock(net.minecraft.world.entity.ai.control.JumpControl.class));
   actor.aiStep();System.out.println("living-ai\t"+(index++)+"\t"+v(velocity)+"\t"+noAi+"\t"+dead+"\t"+simulate+"\t"+v(actor.getDeltaMovement())+"\t"+actor.inputsOut()+"\t"+String.join(";",actor.events));
  }
 }
}
