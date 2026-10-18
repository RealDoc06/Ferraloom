import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.effect.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Actual LivingEntity jump power, parent jump body and liquid impulse. */
public final class JumpVectors {
  public static void run(Level level)throws Exception {
    var power=LivingEntity.class.getDeclaredMethod("getJumpPower",float.class);power.setAccessible(true);
    var liquid=LivingEntity.class.getDeclaredMethod("jumpInLiquid",net.minecraft.tags.TagKey.class);liquid.setAccessible(true);
    var shared=Entity.class.getDeclaredMethod("setSharedFlag",int.class,boolean.class);shared.setAccessible(true);
    float[] multipliers={1,0.5f,-1,0,Float.NaN};
    double[] strengths={0.42,0,1e-6,1e-5,Math.nextUp((double)1e-5f),1.25};
    for(String kind:List.of("player","cow"))for(int i=0;i<24;i++) {
      level.getClass().getDeclaredMethod("floor").invoke(level);
      var surface=i%3==0?Blocks.HONEY_BLOCK:Blocks.STONE;
      ClimbVectors.put(level,new BlockPos(8,63,8),surface.defaultBlockState());
      LivingEntity entity=kind.equals("player")?new TravelVectors(level):new Cow(EntityTypes.COW,level);
      entity.setPos(8.5,64,8.5);entity.setYRot(i*37.25f);entity.getAttribute(Attributes.JUMP_STRENGTH).setBaseValue(strengths[i%strengths.length]);
      int boost=i>=16?(i==23?255:i%3):-1;
      if(boost>=0)entity.addEffect(new MobEffectInstance(MobEffects.JUMP_BOOST,100,boost));
      shared.invoke(entity,3,i%2==0);
      var before=new Vec3(-0.0,i%4==0?0.9:-0.08,0.125);entity.setDeltaMovement(before);entity.needsSync=false;
      var arbitrary=(Float)power.invoke(entity,multipliers[i%multipliers.length]);var ordinary=(Float)power.invoke(entity,1f);
      entity.jumpFromGround();var after=entity.getDeltaMovement();boolean sync=entity.needsSync;
      entity.setDeltaMovement(before);liquid.invoke(entity,FluidTags.WATER);
      System.out.println("living-jump\t"+kind+"\t"+i+"\t"+Block.getId(surface.defaultBlockState())+"\t"+TravelVectors.d(strengths[i%strengths.length])+"\t"+TravelVectors.f(multipliers[i%multipliers.length])+"\t"+boost+"\t"+TravelVectors.f(entity.getYRot())+"\t"+entity.isSprinting()+"\t"+TrackingVectors.vec(before)+"\t"+TravelVectors.f(arbitrary)+"\t"+TravelVectors.f(ordinary)+"\t"+TrackingVectors.vec(after)+"\t"+sync+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+(entity instanceof Mob mob?mob.getNavigation().canFloat():"-"));
    }
    for(boolean canFloat:List.of(false,true))for(var before:List.of(new Vec3(-0.0,-0.0,-0.0),new Vec3(0.12,-0.08,0.03),new Vec3(1,0.9,-2))) {
      var cow=new Cow(EntityTypes.COW,level);cow.getNavigation().setCanFloat(canFloat);cow.setDeltaMovement(before);
      liquid.invoke(cow,FluidTags.WATER);
      System.out.println("mob-liquid-jump\t"+canFloat+"\t"+TrackingVectors.vec(before)+"\t"+TrackingVectors.vec(cow.getDeltaMovement()));
    }
  }
}
