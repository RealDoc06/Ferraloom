import java.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.phys.Vec3;

/** Real relative transform and Entity.teleportSetPosition, including float trig. */
public final class TeleportVectors {
  static String value(PositionMoveRotation p) {
    return TrackingVectors.vec(p.position())+"\t"+TrackingVectors.vec(p.deltaMovement())+"\t"+Integer.toHexString(Float.floatToRawIntBits(p.yRot()))+","+Integer.toHexString(Float.floatToRawIntBits(p.xRot()));
  }
  public static void run(net.minecraft.world.level.Level fixture) {
    var source=new PositionMoveRotation(new Vec3(8.5,64.25,-12.75),new Vec3(0.25,-0.125,0.5),721.25f,-28.125f);
    var change=new PositionMoveRotation(new Vec3(-0.0,0.0625,14),new Vec3(-0.0,0.2,-0.3),-91.5f,120.25f);
    int[] flags={0,1,2,4,8,16,32,64,128,256,511,504,31,255,257,288,384,0x7fffffff};
    for(int bits:flags) {
      var absolute=PositionMoveRotation.calculateAbsolute(source,change,Relative.unpack(bits));
      System.out.println("teleport-transform\t"+bits+"\t"+value(absolute));
      var player=new LivingVectors(fixture);player.setId(1);player.setPos(source.position());player.setYRot(source.yRot());player.setXRot(source.xRot());player.setDeltaMovement(source.deltaMovement());
      player.teleportSetPosition(source,change,Relative.unpack(bits));
      System.out.println("teleport-set\t"+bits+"\t"+value(new PositionMoveRotation(player.position(),player.getDeltaMovement(),player.getYRot(),player.getXRot()))+"\t"+Integer.toHexString(Float.floatToRawIntBits(player.getYHeadRot()))+"\t"+TrackingVectors.vec(new Vec3(player.xo,player.yo,player.zo)));
    }
  }
}
