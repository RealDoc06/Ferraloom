import java.util.UUID;
import com.mojang.authlib.GameProfile;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Executes native input, view, friction and fluid-travel arithmetic. */
public final class TravelVectors extends Player {
  TravelVectors(Level level) {super(level,new GameProfile(new UUID(3,4),"travel"));}
  @Override public GameType gameMode() {return GameType.SURVIVAL;}
  void sprint(boolean value) {setSharedFlag(3,value);}
  float flyingSpeed() {return getFlyingSpeed();}
  static String f(float value) {return Integer.toHexString(Float.floatToRawIntBits(value));}
  static String d(double value) {return Long.toHexString(Double.doubleToRawLongBits(value));}
  public static void run(Level level) throws Exception {
    var player=new TravelVectors(level);
    double threshold=Math.sqrt(1.0e-7);
    Vec3[] inputs={Vec3.ZERO,new Vec3(0.0001,0,0),new Vec3(Math.nextDown(threshold),0,0),
      new Vec3(Math.nextUp(threshold),0,0),new Vec3(1,0,0),new Vec3(1,1,1),
      new Vec3(-0.25,0.125,0.5),new Vec3(0,1,100),new Vec3(-1,0,-1),new Vec3(-0.0,0,-0.0)};
    float[] speeds={0.02f,-0.05f,0.10000001f};float[] yaw={0,90,-721.25f,180,-0.0f};
    float[] pitch={0,-90,37.25f,89.999f,-0.0f};
    for(int i=0;i<inputs.length;i++) {
      float speed=speeds[i%speeds.length];player.setYRot(yaw[i%yaw.length]);player.setXRot(pitch[i%pitch.length]);
      var before=new Vec3(-0.0,0.123,-0.5);player.setDeltaMovement(before);player.needsSync=false;
      player.moveRelative(speed,inputs[i]);
      System.out.println("travel-input\t"+i+"\t"+f(speed)+"\t"+f(player.getYRot())+"\t"+f(player.getXRot())+"\t"+
        TrackingVectors.vec(inputs[i])+"\t"+TrackingVectors.vec(before)+"\t"+TrackingVectors.vec(player.getDeltaMovement())+"\t"+TrackingVectors.vec(player.getLookAngle())+"\t"+player.needsSync);
    }
    var influenced=LivingEntity.class.getDeclaredMethod("getFrictionInfluencedSpeed",float.class);influenced.setAccessible(true);
    var modified=LivingEntity.class.getDeclaredMethod("computeModifiedFriction",float.class,float.class);modified.setAccessible(true);
    float[] friction={0,Math.nextDown(0.6f),0.6f,Math.nextUp(0.6f),0.98f,1,0.54600006f,0.91f};
    float[] modifiers={0,0.5f,1,2,100};
    for(int i=0;i<friction.length*2;i++) {
      boolean grounded=i<friction.length;player.setOnGround(grounded);player.sprint(i%3==0);
      player.getAbilities().flying=i%4==0;player.getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(i%2==0?0.1:1.25);
      float block=friction[i%friction.length];float modifier=modifiers[i%modifiers.length];
      float base=i%2==0?0.91f:0.98f;
      System.out.println("travel-friction\t"+i+"\t"+f(player.getSpeed())+"\t"+f(player.flyingSpeed())+"\t"+grounded+"\t"+f(block)+"\t"+
        f((Float)influenced.invoke(player,block))+"\t"+f(base)+"\t"+f(modifier)+"\t"+f((Float)modified.invoke(null,base,modifier)));
    }
    double[][] fluids={{0.08,0.005},{0.01,0.000625},{0.01,0.003625},{0.01,Math.nextUp(0.003625)},
      {0,-0.0},{-0.08,-0.005},{0.08,-0.02},{0.08,0.5},{0.01,Math.nextDown(-0.002375)}};
    for(int i=0;i<fluids.length;i++) {
      boolean falling=i%3!=0;boolean sprint=i==7;player.sprint(sprint);
      var movement=new Vec3(-0.0,fluids[i][1],0.125);
      System.out.println("travel-fluid-drag\t"+i+"\t"+d(fluids[i][0])+"\t"+falling+"\t"+sprint+"\t"+
        TrackingVectors.vec(movement)+"\t"+TrackingVectors.vec(player.getFluidFallingAdjustedMovement(fluids[i][0],falling,movement)));
    }
  }
}
