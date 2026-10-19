import java.lang.reflect.*;
import java.util.*;
import net.minecraft.world.effect.*;

/** Executes native effect instance constructors, replacement chains and duration
 * steps. World/effect application callbacks are not substituted or executed. */
public final class EffectVectors {
  static Field hidden;
  static String state(MobEffectInstance value)throws Exception {
    if(value==null)return "-";
    return value.getDuration()+":"+value.getAmplifier()+":"+value.isAmbient()+":"+value.isVisible()+":"+value.showIcon()+"/"+state((MobEffectInstance)hidden.get(value));
  }
  static MobEffectInstance instance(int duration,int amplifier,int flags){
    return new MobEffectInstance(MobEffects.SPEED,duration,amplifier,(flags&1)!=0,(flags&2)!=0,(flags&4)!=0);
  }
  public static void run()throws Exception {
    hidden=MobEffectInstance.class.getDeclaredField("hiddenEffect");hidden.setAccessible(true);
    var tick=MobEffectInstance.class.getDeclaredMethod("tickDownDuration");tick.setAccessible(true);
    var downgrade=MobEffectInstance.class.getDeclaredMethod("downgradeToHiddenEffect");downgrade.setAccessible(true);
    var remaining=MobEffectInstance.class.getDeclaredMethod("hasRemainingDuration");remaining.setAccessible(true);
    int[] durations={0,1,2,100,-1,-2,Integer.MIN_VALUE,Integer.MAX_VALUE};
    int[] amplifiers={-7,0,2,300};
    for(int i=0;i<durations.length*amplifiers.length;i++){
      int duration=durations[i/amplifiers.length],amplifier=amplifiers[i%amplifiers.length],flags=i%8;
      var value=instance(duration,amplifier,flags);
      System.out.println("mob-effect-instance\t"+duration+"\t"+amplifier+"\t"+flags+"\t"+state(value)+"\t"+value.endsWithin(0)+"\t"+remaining.invoke(value));
      var takeover=instance(durations[(i+3)%durations.length],amplifiers[(i+1)%amplifiers.length],(flags+3)%8);
      String setup=takeover.getDuration()+"\t"+takeover.getAmplifier()+"\t"+((flags+3)%8);
      boolean changed=value.update(takeover);
      System.out.println("mob-effect-update\t"+duration+"\t"+amplifier+"\t"+flags+"\t"+setup+"\t"+changed+"\t"+state(value));
    }
    var value=instance(100,0,6);
    int[][] updates={{20,2,0},{200,1,3},{30,2,7},{40,3,6},{400,0,5}};
    for(int i=0;i<updates.length;i++){
      var setup=updates[i];boolean changed=value.update(instance(setup[0],setup[1],setup[2]));
      System.out.println("mob-effect-chain\t"+i+"\t"+setup[0]+"\t"+setup[1]+"\t"+setup[2]+"\t"+changed+"\t"+state(value));
    }
    var copy=new MobEffectInstance(value);
    System.out.println("mob-effect-copy\t"+state(copy)+"\t"+value.equals(copy));
    for(float scale:new float[]{0,0.125f,-1,Float.NaN,Float.POSITIVE_INFINITY,Float.NEGATIVE_INFINITY})
      System.out.println("mob-effect-scale\t"+Integer.toHexString(Float.floatToRawIntBits(scale))+"\t"+state(value.withScaledDuration(scale)));
    for(int i=1;i<=42;i++){
      tick.invoke(value);boolean changed=(boolean)downgrade.invoke(value);
      if(i==1 || changed || i==42)System.out.println("mob-effect-duration\t"+i+"\t"+changed+"\t"+state(value));
    }
    for(int duration:new int[]{-1,0,-2,Integer.MIN_VALUE}){
      value=instance(duration,0,6);tick.invoke(value);
      System.out.println("mob-effect-negative\t"+duration+"\t"+state(value)+"\t"+state(value.withScaledDuration(0.5f)));
    }
  }
}
