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
    for(float[] rotation:List.of(new float[]{721.25f,270f},new float[]{-721.25f,-360f},new float[]{37.25f,-28.125f})) {
      var player=new LivingVectors(fixture);player.setPos(8.5,64.25,-12.75);player.setOldPosAndRot();
      player.absSnapRotationTo(net.minecraft.util.Mth.wrapDegrees(rotation[0]),net.minecraft.util.Mth.wrapDegrees(rotation[1]));
      System.out.println("teleport-pending-rotation\t"+Integer.toHexString(Float.floatToRawIntBits(rotation[0]))+","+Integer.toHexString(Float.floatToRawIntBits(rotation[1]))+"\t"+Integer.toHexString(Float.floatToRawIntBits(player.getYRot()))+","+Integer.toHexString(Float.floatToRawIntBits(player.getXRot()))+","+Integer.toHexString(Float.floatToRawIntBits(player.yRotO))+","+Integer.toHexString(Float.floatToRawIntBits(player.xRotO))+"\t"+TrackingVectors.vec(new Vec3(player.xo,player.yo,player.zo)));
    }
    for(Vec3 destination:List.of(new Vec3(8.5,64,8.5),new Vec3(3.0e7+1,65,-3.0e7-1))) {
      var player=new LivingVectors(fixture);
      player.teleportSetPosition(source,new PositionMoveRotation(destination,Vec3.ZERO,721.25f,-28.125f),Set.of());
      player.absSnapTo(destination.x,destination.y,destination.z,player.getYRot(),player.getXRot());
      System.out.println("teleport-confirm-snap\t"+TrackingVectors.vec(destination)+"\t"+TrackingVectors.vec(player.position())+"\t"+TrackingVectors.vec(new Vec3(player.xo,player.yo,player.zo))+"\t"+TrackingVectors.vec(player.oldPosition())+"\t"+Integer.toHexString(Float.floatToRawIntBits(player.getYRot()))+","+Integer.toHexString(Float.floatToRawIntBits(player.getYHeadRot()))+","+Integer.toHexString(Float.floatToRawIntBits(player.yRotO)));
    }
    for(var type:net.minecraft.world.level.GameType.values()) for(boolean flying:List.of(false,true)) {
      var player=new LivingVectors(fixture);player.getAbilities().flying=flying;
      type.updatePlayerAbilities(player.getAbilities());
      boolean mayBuild=player.mayBuild();boolean resultFlying=player.getAbilities().flying;
      player.getAbilities().mayBuild=!mayBuild;
      var saved=net.minecraft.world.entity.player.Abilities.Packed.CODEC.encodeStart(net.minecraft.nbt.NbtOps.INSTANCE,player.getAbilities().pack()).getOrThrow().asCompound().orElseThrow();
      System.out.println("player-may-build\t"+type.getId()+"\t"+flying+"\t"+mayBuild+","+resultFlying+","+player.mayBuild()+","+saved.getBooleanOr("mayBuild",true));
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
