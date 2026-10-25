import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.stats.Stat;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real Living/Player causeFallDamage, with synchronous virtual sound, stat and
 * hurt callbacks captured instead of entering the already-covered hurt pipeline. */
public final class FallDamageVectors {
  static String d(double n){return Long.toHexString(Double.doubleToRawLongBits(n));}
  static String f(float n){return Integer.toHexString(Float.floatToRawIntBits(n));}
  static final List<String> events=new ArrayList<>();
  static String state(LivingEntity e) {
    try {
      var impact=(Vec3)ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").get(e);
      return (impact==null?"none":d(impact.y))+","+ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").getInt(e);
    }catch(Exception error){throw new RuntimeException(error);}
  }
  static void sound(SoundEvent sound,float volume,float pitch){events.add("sound:"+sound.location()+":"+f(volume)+":"+f(pitch));}
  static boolean hurt(LivingEntity entity,DamageSource source,float amount){events.add("hurt:"+source.typeHolder().unwrapKey().orElseThrow().identifier()+":"+f(amount)+":"+state(entity));return false;}
  static class CowReceiver extends Cow {
    CowReceiver(Level level){super(EntityTypes.COW,level);}
    int calculate(double distance,float modifier){return super.calculateFallDamage(distance,modifier);}
    @Override public void playSound(SoundEvent sound,float volume,float pitch){sound(sound,volume,pitch);}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount){return FallDamageVectors.hurt(this,source,amount);}
  }
  static class PlayerReceiver extends Player {
    PlayerReceiver(Level level){super(level,new GameProfile(new UUID(1,2),"fall"));}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public void awardStat(Stat<?> stat,int amount){events.add("stat:"+stat.getName()+":"+amount);}
    @Override public void playSound(SoundEvent sound,float volume,float pitch){sound(sound,volume,pitch);}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount){return FallDamageVectors.hurt(this,source,amount);}
  }
  public static void run(Level original)throws Exception {
    var server=mock(ServerLevel.class);
    when(server.getBlockState(any(BlockPos.class))).thenAnswer(call->original.getBlockState(call.getArgument(0)));
    double[] distances={Double.NEGATIVE_INFINITY,-1,-0.0,0,1.999999,2,2.999998,2.999999,3,3.25,4,6,9.25,20,Double.POSITIVE_INFINITY,Double.NaN};
    float[] modifiers={0,0.2F,1,1.5F,-1,Float.POSITIVE_INFINITY,Float.NaN};
    double[] safes={0,3,9.25};double[] multipliers={0,0.125,1,2.5};
    for(int i=0;i<80;i++) {
      var e=new CowReceiver(original);double distance=distances[i%distances.length];float modifier=modifiers[(i/3)%modifiers.length];double safe=safes[(i/5)%safes.length],multiplier=multipliers[(i/7)%multipliers.length];
      e.getAttribute(Attributes.SAFE_FALL_DISTANCE).setBaseValue(safe);e.getAttribute(Attributes.FALL_DAMAGE_MULTIPLIER).setBaseValue(multiplier);
      System.out.println("fall-calculate\t"+d(distance)+"\t"+f(modifier)+"\t"+d(safe)+"\t"+d(multiplier)+"\t"+e.calculate(distance,modifier));
    }
    for(String kind:List.of("cow","player"))for(int i=0;i<32;i++) {
      LivingEntity e=kind.equals("player")?new PlayerReceiver(original):new CowReceiver(original);
      e.setPos(8.5,64,8.5);ExperienceVectors.field(Entity.class,"level").set(e,server);
      double distance=distances[i%distances.length];float modifier=modifiers[(i/4)%modifiers.length];
      double safe=safes[(i/5)%safes.length],multiplier=multipliers[(i/6)%multipliers.length];
      e.getAttribute(Attributes.SAFE_FALL_DISTANCE).setBaseValue(safe);e.getAttribute(Attributes.FALL_DAMAGE_MULTIPLIER).setBaseValue(multiplier);
      int impulse=i%5;int grace=impulse==1?40:impulse==2?-1:0;Double impact=impulse==0?null:impulse==3?63.0:impulse==4?64.0:70.0;
      if(impact!=null){e.setIgnoreFallDamageFromCurrentImpulse(true,new Vec3(8.5,impact,8.5));ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").setInt(e,grace);}
      boolean silent=i%4==0,mayfly=i%7==0,air=i%3==0;e.setSilent(silent);
      if(e instanceof Player player)player.getAbilities().mayfly=mayfly;
      var pos=new BlockPos(8,63,8);original.getChunkAt(pos).getSection(original.getSectionIndex(63)).setBlockState(8,15,8,(air?Blocks.AIR:Blocks.STONE).defaultBlockState());
      var source=i%2==0?original.damageSources().fall():original.damageSources().drown();events.clear();
      boolean result=e.causeFallDamage(distance,modifier,source);
      System.out.println("fall-apply\t"+kind+"\t"+i+"\t"+d(distance)+"\t"+f(modifier)+"\t"+d(safe)+"\t"+d(multiplier)+"\t"+(impact==null?"none":d(impact))+"\t"+grace+"\t"+silent+"\t"+mayfly+"\t"+air+"\t"+result+"\t"+state(e)+"\t"+(events.isEmpty()?"-":String.join(";",events)));
    }
    for(int grace:List.of(0,40)) {
      var parent=new CowReceiver(original);var child=new CowReceiver(original);
      parent.setPos(8.5,64,8.5);child.setPos(8.5,64,8.5);
      child.getAttribute(Attributes.SAFE_FALL_DISTANCE).setBaseValue(100);
      child.setIgnoreFallDamageFromCurrentImpulse(true,new Vec3(8.5,69,8.5));
      ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").setInt(child,grace);
      ExperienceVectors.field(Entity.class,"level").set(parent,server);ExperienceVectors.field(Entity.class,"level").set(child,server);
      if(!child.startRiding(parent,true,true))throw new AssertionError("mount");events.clear();
      boolean result=parent.causeFallDamage(6,1,original.damageSources().drown());
      System.out.println("fall-passengers\t"+grace+"\t"+result+"\t"+state(child)+"\t"+String.join(";",events));
    }
  }
}
