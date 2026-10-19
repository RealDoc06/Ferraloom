import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Calls the actual virtual Player/LivingEntity.travel entry, including rider,
 * swimming steering and flight restoration around the parent body. */
public final class TravelDispatchVectors {
  public static void run(Level level)throws Exception {
    var refresh=Entity.class.getDeclaredMethod("updateFluidInteraction");refresh.setAccessible(true);
    var setFlag=Entity.class.getDeclaredMethod("setSharedFlag",int.class,boolean.class);setFlag.setAccessible(true);
    for(String kind:List.of("player","cow"))for(int i=0;i<23;i++) {
      level.getClass().getDeclaredMethod("floor").invoke(level);
      boolean water=i>=3&&i<=9||i==11||i==13, lava=i==16||i==17;
      var surface=i==2?Blocks.ICE:i==18||i==19?Blocks.SOUL_SAND:i>=20?Blocks.HONEY_BLOCK:Blocks.STONE;
      ClimbVectors.put(level,new BlockPos(8,63,8),surface.defaultBlockState());
      var fluid=water?Blocks.WATER:lava?Blocks.LAVA:Blocks.AIR;
      int top=i==6||i==7||i==17?64:66;
      for(int x=6;x<=11;x++)for(int y=64;y<=68;y++)for(int z=6;z<=11;z++)
        ClimbVectors.put(level,new BlockPos(x,y,z),y<=top?fluid.defaultBlockState():Blocks.AIR.defaultBlockState());
      if(i==15)ClimbVectors.put(level,new BlockPos(8,64,8),Blocks.LADDER.defaultBlockState());
      LivingEntity entity=kind.equals("player")?new TravelVectors(level):new Cow(EntityTypes.COW,level);
      boolean ground=i==0||i==2||i==18||i==19||i==21||i==22;
      var position=new Vec3(8.5,ground?64:i==6||i==7?64.1:i==17?64.75:water||lava||i==15||i==20?64.5:68.5,8.5);
      var before=new Vec3(0.12,i==7?0.02:-0.08,0.03);var input=new Vec3(0.6,0.2,-0.3);
      entity.setPos(position);entity.setDeltaMovement(before);entity.setYRot(37.25f);entity.setXRot(i==4?45:i>=5&&i<=7?-45:i==8?-5:0);entity.setOnGround(ground);entity.fallDistance=0;
      entity.setSpeed(0.17f);entity.getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.17);
      boolean swimming=i>=4&&i<=9||i==13, flying=i==9||i==10||i==11||i==12||i==19||i==22;
      entity.setSwimming(swimming);if(entity instanceof TravelVectors player){player.getAbilities().flying=flying;player.getAbilities().setFlyingSpeed(0.125f);}
      ExperienceVectors.field(LivingEntity.class,"jumping").setBoolean(entity,i==7);
      boolean riding=i==12||i==13;if(riding)entity.startRiding(new PrimedTnt(EntityTypes.TNT,level),true,false);
      setFlag.invoke(entity,7,i==14||i==15||i==20);refresh.invoke(entity);ExperienceVectors.field(Entity.class,"firstTick").setBoolean(entity,false);
      String setup=kind+"\t"+i+"\t"+Block.getId(surface.defaultBlockState())+"\t"+TrackingVectors.vec(position)+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+TrackingVectors.vec(input)+"\t"+TravelVectors.f(entity.getYRot())+"\t"+TravelVectors.f(entity.getXRot())+"\t"+entity.onGround()+"\t"+swimming+"\t"+(flying&&kind.equals("player"))+"\t"+TravelVectors.f(0.125f)+"\t"+riding+"\t"+(i==14||i==15||i==20)+"\t"+(i==7);
      entity.travel(input);
      System.out.println("travel-dispatch\t"+setup+"\t"+TrackingVectors.vec(entity.position())+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+AirTravelVectors.flags(entity)+"\t"+TravelVectors.d(entity.fallDistance)+"\t"+entity.isFallFlying());
    }
  }
}
