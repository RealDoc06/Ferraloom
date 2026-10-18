import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.effect.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Actual fall-flying bodies and collision callback order. The isolated hit
 * receiver records sound/hurt virtuals without substituting the collision body. */
public final class FallFlyingVectors {
  static final class ServerFixture extends ServerLevel {
    DamageSources sources;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public DamageSources damageSources(){return sources;}
  }
  static final class CowHits extends Cow {
    final StringJoiner calls=new StringJoiner(",");
    CowHits(Level level){super(EntityTypes.COW,level);}
    @Override public void playSound(SoundEvent sound,float volume,float pitch){calls.add("sound:"+sound.location()+":"+TravelVectors.f(volume)+":"+TravelVectors.f(pitch));}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float damage){calls.add("hurt:"+source.type().msgId()+":"+TravelVectors.f(damage));return false;}
  }
  static final class PlayerHits extends net.minecraft.world.entity.player.Player {
    final StringJoiner calls=new StringJoiner(",");
    PlayerHits(Level level){super(level,new com.mojang.authlib.GameProfile(new UUID(3,4),"flight"));}
    @Override public net.minecraft.world.level.GameType gameMode(){return net.minecraft.world.level.GameType.SURVIVAL;}
    @Override public void playSound(SoundEvent sound,float volume,float pitch){calls.add("sound:"+sound.location()+":"+TravelVectors.f(volume)+":"+TravelVectors.f(pitch));}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float damage){calls.add("hurt:"+source.type().msgId()+":"+TravelVectors.f(damage));return false;}
  }
  public static void run(Level level)throws Exception {
    var update=LivingEntity.class.getDeclaredMethod("updateFallFlyingMovement",Vec3.class);update.setAccessible(true);
    var travel=LivingEntity.class.getDeclaredMethod("travelFallFlying",Vec3.class);travel.setAccessible(true);
    var hit=LivingEntity.class.getDeclaredMethod("handleFallFlyingCollisions",double.class,double.class);hit.setAccessible(true);
    var setFlag=Entity.class.getDeclaredMethod("setSharedFlag",int.class,boolean.class);setFlag.setAccessible(true);
    float[] pitches={0,30,-30,89.999f,-89.999f,37.25f,-0.0f,-1e-7f};
    float[] yaws={0,37.25f,90,180,-721.25f};
    Vec3[] velocities={new Vec3(0.4,-0.08,0.125),new Vec3(-0.0,0.2,0.03),new Vec3(-1.7,-1.2,0.25),Vec3.ZERO};
    for(int i=0;i<40;i++) {
      var player=new TravelVectors(level);player.setYRot(yaws[i%yaws.length]);player.setXRot(pitches[i%pitches.length]);
      var before=velocities[i%velocities.length];player.setDeltaMovement(before);player.setNoGravity(i%11==0);
      player.getAttribute(Attributes.GRAVITY).setBaseValue(i%9==0?0.005:0.08);
      boolean slow=i%7==0;if(slow)player.addEffect(new MobEffectInstance(MobEffects.SLOW_FALLING,100));
      var after=(Vec3)update.invoke(player,before);
      System.out.println("fall-flying-update\t"+i+"\t"+TravelVectors.f(player.getYRot())+"\t"+TravelVectors.f(player.getXRot())+"\t"+TrackingVectors.vec(before)+"\t"+TravelVectors.d(player.getAttributeValue(Attributes.GRAVITY))+"\t"+player.isNoGravity()+"\t"+slow+"\t"+TrackingVectors.vec(after));
    }
    for(String kind:List.of("player","cow"))for(int i=0;i<12;i++) {
      level.getClass().getDeclaredMethod("floor").invoke(level);
      var blockPos=new BlockPos(8,64,8);ClimbVectors.put(level,blockPos,i>=10?Blocks.LADDER.defaultBlockState():Blocks.AIR.defaultBlockState());
      LivingEntity entity=kind.equals("player")?new TravelVectors(level):new Cow(EntityTypes.COW,level);
      var position=new Vec3(8.5,i>=10?64.7:68.5,8.5);var before=velocities[i%velocities.length];var input=new Vec3(0.6,0,-0.3);
      entity.setPos(position);entity.setYRot(yaws[i%yaws.length]);entity.setXRot(pitches[i%pitches.length]);entity.setDeltaMovement(before);entity.fallDistance=0;
      entity.setNoGravity(i==4);entity.getAttribute(Attributes.GRAVITY).setBaseValue(i==8?0.005:0.08);entity.setSpeed(0.17f);entity.getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.17);
      boolean slow=i==6;if(slow)entity.addEffect(new MobEffectInstance(MobEffects.SLOW_FALLING,100));
      setFlag.invoke(entity,7,i!=11);entity.getEntityData().packDirty();
      String setup=kind+"\t"+i+"\t"+TrackingVectors.vec(position)+"\t"+TrackingVectors.vec(before)+"\t"+TrackingVectors.vec(input)+"\t"+TravelVectors.f(entity.getYRot())+"\t"+TravelVectors.f(entity.getXRot())+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.GRAVITY))+"\t"+entity.isNoGravity()+"\t"+slow+"\t"+(i>=10)+"\t"+(i!=11);
      travel.invoke(entity,input);
      System.out.println("travel-fall-flying\t"+setup+"\t"+TrackingVectors.vec(entity.position())+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+AirTravelVectors.flags(entity)+"\t"+TravelVectors.d(entity.fallDistance)+"\t"+entity.isFallFlying()+"\t"+entity.getEntityData().isDirty());
    }
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    double[][] speeds={{0.2,0},{0.3,0},{Math.nextUp(0.3),0},{0.7,0},{0.8,0},{Math.nextDown(0.8),0},{1.25,0.125},{Double.NaN,0},{Double.POSITIVE_INFINITY,0},{1,Double.POSITIVE_INFINITY},{1,0.1},{10,0}};
    for(String kind:List.of("player","cow"))for(int i=0;i<speeds.length;i++) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);server.sources=level.damageSources();
      LivingEntity entity=kind.equals("player")?new PlayerHits(level):new CowHits(level);ExperienceVectors.field(Entity.class,"level").set(entity,server);
      entity.horizontalCollision=i!=11;hit.invoke(entity,speeds[i][0],speeds[i][1]);
      var calls=entity instanceof PlayerHits p?p.calls:((CowHits)entity).calls;
      System.out.println("fall-flying-hit\t"+kind+"\t"+i+"\t"+entity.horizontalCollision+"\t"+TravelVectors.d(speeds[i][0])+"\t"+TravelVectors.d(speeds[i][1])+"\t"+(calls.length()==0?"-":calls));
    }
  }
}
