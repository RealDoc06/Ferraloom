import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.player.Player;
import static org.mockito.Mockito.*;

/** Native Mob speed callback ordering and inherited Living control setters. */
public final class MobAccessorVectors {
 static String f(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
 static final class Receiver extends Cow {
  final List<String> events=new ArrayList<>();boolean remove;
  Receiver(ServerLevel level){super(EntityTypes.COW,level);}
  @Override public void setZza(float value){
   events.add("zza:"+f(getSpeed())+":"+f(this.zza));
   if(remove)discard();
   super.setZza(value);
  }
  String values(){return f(getSpeed())+","+f(this.zza)+","+this.jumping+","+f(this.yHeadRot)+","+f(this.yBodyRot)+","+f(this.xxa)+","+f(this.yya);}
 }
 static float speed(LivingEntity entity)throws Exception{var field=LivingEntity.class.getDeclaredField("speed");field.setAccessible(true);return field.getFloat(entity);}
 static void oldSpeed(LivingEntity entity)throws Exception{var field=LivingEntity.class.getDeclaredField("speed");field.setAccessible(true);field.setFloat(entity,0.25f);}
 static boolean jumping(LivingEntity entity)throws Exception{var field=LivingEntity.class.getDeclaredField("jumping");field.setAccessible(true);return field.getBoolean(entity);}
 public static void main(String[] args)throws Exception{
  var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
  var registries=NeighborVectors.load();var level=mock(ServerLevel.class,RETURNS_DEEP_STUBS);when(level.registryAccess()).thenReturn(registries);
  for(float value:new float[]{0f,-0f,1f,-1f,0.125f,Float.intBitsToFloat(0x7fc01234),Float.POSITIVE_INFINITY,Float.NEGATIVE_INFINITY}) {
   for(boolean remove:List.of(false,true)) {
    var actor=new Receiver(level);oldSpeed(actor);actor.zza=-0.5f;actor.setJumping(true);actor.remove=remove;
    actor.setSpeed(value);actor.setJumping(value!=0f);actor.setYHeadRot(value);actor.setYBodyRot(-value);actor.setXxa(value);actor.setYya(-value);
    System.out.println("mob-accessor\t"+f(value)+"\t"+remove+"\t"+String.join(";",actor.events)+"\t"+actor.values()+"\t"+actor.isRemoved()+"\t"+actor.getMaxHeadXRot()+","+actor.getMaxHeadYRot()+","+actor.getHeadRotSpeed());
   }
   var player=mock(Player.class,CALLS_REAL_METHODS);oldSpeed(player);player.zza=-0.5f;player.setJumping(true);
   player.setSpeed(value);player.setJumping(value!=0f);player.setYHeadRot(value);player.setYBodyRot(-value);
   System.out.println("living-accessor\t"+f(value)+"\t"+f(speed(player))+","+f(player.zza)+","+jumping(player)+","+f(player.getYHeadRot())+","+f(player.yBodyRot));
  }
 }
}
