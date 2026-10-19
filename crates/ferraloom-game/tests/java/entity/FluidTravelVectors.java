import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.effect.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Runs the native complete fluid and flying bodies after their actual fluid
 * interaction update, with controlled fluid depth and collision exit probes. */
public final class FluidTravelVectors {
  static void prepare(Level level, String kind, int test)throws Exception {
    level.getClass().getDeclaredMethod("floor").invoke(level);
    var fluid=kind.equals("water")?Blocks.WATER:kind.equals("lava")?Blocks.LAVA:Blocks.AIR;
    int top=test==1||test==5||test==6?64:66;
    for(int x=6;x<=11;x++)for(int y=64;y<=68;y++)for(int z=6;z<=11;z++)
      ClimbVectors.put(level,new BlockPos(x,y,z),y<=top?fluid.defaultBlockState():Blocks.AIR.defaultBlockState());
    if(test==5||test==6||test==7||test==8) {
      int wallTop=test==6?67:64;
      for(int y=64;y<=wallTop;y++)for(int z=6;z<=11;z++)ClimbVectors.put(level,new BlockPos(9,y,z),Blocks.STONE.defaultBlockState());
      if(test==7)ClimbVectors.put(level,new BlockPos(8,64,8),Blocks.LADDER.defaultBlockState());
    }
  }
  public static void run(Level level)throws Exception {
    var fluidTravel=LivingEntity.class.getDeclaredMethod("travelInFluid",Vec3.class);fluidTravel.setAccessible(true);
    var flyingTravel=LivingEntity.class.getDeclaredMethod("travelFlying",Vec3.class,float.class,float.class,float.class);flyingTravel.setAccessible(true);
    var refresh=Entity.class.getDeclaredMethod("updateFluidInteraction");refresh.setAccessible(true);
    var should=LivingEntity.class.getDeclaredMethod("shouldTravelInFluid",net.minecraft.world.level.material.FluidState.class);should.setAccessible(true);
    var first=ExperienceVectors.field(Entity.class,"firstTick");
    for(String type:List.of("player","cow"))for(String kind:List.of("water","lava","air"))for(int test=0;test<(kind.equals("air")?3:16);test++){
      prepare(level,kind,test);
      if(test==6)ClimbVectors.put(level,new BlockPos(8,66,8),type.equals("player")?Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,net.minecraft.world.level.block.state.properties.SlabType.TOP):Blocks.STONE.defaultBlockState());
      LivingEntity entity=type.equals("player")?new TravelVectors(level):new Cow(EntityTypes.COW,level);
      var position=new Vec3(8.5,test==1?64.75:test==5||test==6?64.5:64,8.5);
      var before=new Vec3(test>=5&&test<=8?0.35:0.12,test==3?0.2:test==4?0:-0.08,0.03);
      var input=new Vec3(0.6,0,-0.3);
      entity.setPos(position);entity.setYRot(0);entity.setDeltaMovement(before);entity.setOnGround(test==2||test==9||test==11);
      entity.setNoGravity(test==4);entity.fallDistance=4;
      entity.getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.17);entity.setSpeed(0.17f);
      entity.getAttribute(Attributes.WATER_MOVEMENT_EFFICIENCY).setBaseValue(test==9||test==10?0.5:test==11||test==12?1:0);
      entity.getAttribute(Attributes.GRAVITY).setBaseValue(test==14?0.005:0.08);
      entity.setSprinting(test==2||test==12);
      if(entity instanceof TravelVectors player)player.getAbilities().flying=test==15;
      int effectId=test==3?BuiltInRegistries.MOB_EFFECT.getId(MobEffects.SLOW_FALLING.value()):test==13?BuiltInRegistries.MOB_EFFECT.getId(MobEffects.DOLPHINS_GRACE.value()):-1;
      if(effectId>=0)entity.addEffect(new MobEffectInstance(test==3?MobEffects.SLOW_FALLING:MobEffects.DOLPHINS_GRACE,100,0));
      refresh.invoke(entity);first.setBoolean(entity,false);
      var actualBefore=entity.getDeltaMovement();
      boolean shouldTravel=(Boolean)should.invoke(entity,level.getFluidState(entity.blockPosition()));
      boolean flying=kind.equals("air")||test==15;
      String setup=type+"\t"+kind+"\t"+test+"\t"+TrackingVectors.vec(position)+"\t"+TrackingVectors.vec(before)+"\t"+TrackingVectors.vec(actualBefore)+"\t"+TrackingVectors.vec(input)+"\t"+entity.onGround()+"\t"+entity.isNoGravity()+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.WATER_MOVEMENT_EFFICIENCY))+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.GRAVITY))+"\t"+TravelVectors.f(entity.getSpeed())+"\t"+entity.isSprinting()+"\t"+(test==15&&type.equals("player"))+"\t"+effectId+"\t"+TravelVectors.d(entity.getFluidHeight(FluidTags.LAVA))+"\t"+TravelVectors.d(entity.getFluidJumpThreshold())+"\t"+shouldTravel+"\t"+flying;
      if(flying)flyingTravel.invoke(entity,input,0.03125f,0.0625f,0.125f);else fluidTravel.invoke(entity,input);
      System.out.println("travel-fluid\t"+setup+"\t"+TrackingVectors.vec(entity.position())+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+AirTravelVectors.flags(entity)+"\t"+TravelVectors.d(entity.fallDistance));
    }
    for(float eye:new float[]{Math.nextDown(0.4f),0.4f,Math.nextUp(0.4f),Float.NaN}){
      var entity=new TravelVectors(level);ExperienceVectors.field(Entity.class,"eyeHeight").setFloat(entity,eye);
      System.out.println("fluid-jump-threshold\t"+TravelVectors.f(eye)+"\t"+TravelVectors.d(entity.getFluidJumpThreshold()));
    }
  }
}
