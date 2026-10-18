import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.effect.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Executes actual LivingEntity.travelInAir, including its Entity.move and
 * contextual collision queries. Each row records its controlled native inputs. */
public final class AirTravelVectors {
  static String flags(Entity e){return (e.onGround()?"1":"0")+(e.horizontalCollision?"1":"0")+(e.verticalCollision?"1":"0")+(e.verticalCollisionBelow?"1":"0")+(e.minorHorizontalCollision?"1":"0");}
  public static void run(Level level)throws Exception {
    var travel=LivingEntity.class.getDeclaredMethod("travelInAir",Vec3.class);travel.setAccessible(true);
    var pos=new BlockPos(8,64,8);
    for(String kind:List.of("player","cow"))for(int i=0;i<21;i++){
      Block surface=i==2||i==9?Blocks.ICE:i==3?Blocks.BLUE_ICE:i==16?Blocks.SOUL_SAND:i==17?Blocks.HONEY_BLOCK:Blocks.STONE;
      // Reuse the concrete Level's native chunk-backed floor and shape services.
      level.getClass().getDeclaredMethod("floor").invoke(level);
      ClimbVectors.put(level,pos.below(),surface.defaultBlockState());ClimbVectors.put(level,pos,i==11||i==12?Blocks.LADDER.defaultBlockState():Blocks.AIR.defaultBlockState());
      LivingEntity entity=kind.equals("player")?new TravelVectors(level):new Cow(EntityTypes.COW,level);
      boolean ground=i==0||i==2||i==3||i==9||i==10||i==16||i==17;
      boolean jumping=i==12||i==13||i==14, powder=i==13||i==14, boots=i==13;
      var position=new Vec3(8.5,ground?64:64.7,8.5);var before=new Vec3(0.12,i==7?0.2:-0.08,0.03);var input=new Vec3(0.6,0,-0.3);
      entity.setPos(position);entity.setYRot(37.25f);entity.setOnGround(ground);entity.setDeltaMovement(before);entity.fallDistance=0;
      entity.setNoGravity(i==4);entity.setDiscardFriction(i==9);
      ExperienceVectors.field(LivingEntity.class,"jumping").setBoolean(entity,jumping);entity.wasInPowderSnow=powder;
      if(boots)entity.setItemSlot(EquipmentSlot.FEET,new ItemStack(Items.LEATHER_BOOTS));
      entity.getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.17);entity.setSpeed(0.17f);
      entity.getAttribute(Attributes.FRICTION_MODIFIER).setBaseValue(i==10?2:1);
      entity.getAttribute(Attributes.AIR_DRAG_MODIFIER).setBaseValue(i==10?0.5:i==19?100:1);
      entity.getAttribute(Attributes.GRAVITY).setBaseValue(i==8?0:i==20?0.005:0.08);
      if(entity instanceof TravelVectors player){player.getAbilities().flying=i==15;player.getAbilities().setFlyingSpeed(0.125f);player.sprint(i==15);}
      int effectId=i==5?BuiltInRegistries.MOB_EFFECT.getId(MobEffects.LEVITATION.value()):i==6||i==7?BuiltInRegistries.MOB_EFFECT.getId(MobEffects.SLOW_FALLING.value()):-1;
      if(effectId>=0)entity.addEffect(new MobEffectInstance(i==5?MobEffects.LEVITATION:MobEffects.SLOW_FALLING,100,i==5?2:0));
      String setup=kind+"\t"+i+"\t"+Block.getId(surface.defaultBlockState())+"\t"+ground+"\t"+TrackingVectors.vec(position)+"\t"+TrackingVectors.vec(before)+"\t"+TrackingVectors.vec(input)+"\t"+TravelVectors.f(entity.getYRot())+"\t"+entity.isNoGravity()+"\t"+entity.shouldDiscardFriction()+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.FRICTION_MODIFIER))+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.AIR_DRAG_MODIFIER))+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.GRAVITY))+"\t"+TravelVectors.f(entity.getSpeed())+"\t"+(i==15&&kind.equals("player"))+"\t"+TravelVectors.f(0.125f)+"\t"+(i==15&&kind.equals("player"))+"\t"+jumping+"\t"+powder+"\t"+boots+"\t"+effectId+"\t"+(i==5?2:0);
      travel.invoke(entity,input);
      System.out.println("travel-air\t"+setup+"\t"+TrackingVectors.vec(entity.position())+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+flags(entity)+"\t"+TravelVectors.d(entity.fallDistance));
    }
    ClimbVectors.put(level,pos,Blocks.AIR.defaultBlockState());ClimbVectors.put(level,pos.below(),Blocks.STONE.defaultBlockState());
  }
}
