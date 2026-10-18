import java.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.phys.Vec3;

/** Real relative transform and Entity.teleportSetPosition, including float trig. */
public final class TeleportVectors {
  static String value(PositionMoveRotation p) {
    return TrackingVectors.vec(p.position())+"\t"+TrackingVectors.vec(p.deltaMovement())+"\t"+Integer.toHexString(Float.floatToRawIntBits(p.yRot()))+","+Integer.toHexString(Float.floatToRawIntBits(p.xRot()));
  }
  public static void run(net.minecraft.world.level.Level fixture) throws Exception {
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
    var root=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,fixture);
    var child=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,fixture);
    var grandchild=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,fixture);
    var entities=List.of(root,child,grandchild);
    for(int index=0;index<entities.size();index++) {
      Entity entity=entities.get(index);entity.setId(index+1);entity.setPos(8.5,64+index*5,8.5);entity.setYRot(index*37.25f);entity.setXRot(index*-14.5f);entity.setDeltaMovement(0.25,-0.125,0.5);
    }
    child.startRiding(root,true,false);grandchild.startRiding(child,true,false);
    root.snapTo(10.5,66,12.5);
    var teleportPassengers=Entity.class.getDeclaredMethod("teleportPassengers");teleportPassengers.setAccessible(true);teleportPassengers.invoke(root);
    for(Entity entity:entities) System.out.println("teleport-passengers\t"+entity.getId()+"\t"+TrackingVectors.vec(entity.position())+"\t"+TrackingVectors.vec(new Vec3(entity.xo,entity.yo,entity.zo))+"\t"+TrackingVectors.vec(entity.getDeltaMovement())+"\t"+Integer.toHexString(Float.floatToRawIntBits(entity.getYRot()))+","+Integer.toHexString(Float.floatToRawIntBits(entity.getXRot())));
  }
}
