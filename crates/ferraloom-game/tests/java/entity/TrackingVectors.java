import java.util.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Executes vanilla's movement encoders and server interpolation tracker. */
public final class TrackingVectors {
  static String bits(double n) { return Long.toHexString(Double.doubleToRawLongBits(n)); }
  static String vec(Vec3 p) { return bits(p.x)+","+bits(p.y)+","+bits(p.z); }
  static String delta(VecDelta d) {
    if(d==null) return "null";
    if(d instanceof VecDelta.Linear e) return e.xa()+","+e.ya()+","+e.za();
    return String.join(";",((VecDelta.Stepped)d).steps().stream().map(e->e.ticks()+":"+e.xa()+","+e.ya()+","+e.za()).toList());
  }
  static String path(PositionPath p) {
    if(p instanceof PositionPath.Linear e) return "L:"+vec(e.endPosition());
    return "S:"+String.join(";",((PositionPath.Stepped)p).steps().stream().map(e->e.tickOffset()+":"+vec(e.position())).toList());
  }
  public static void run(Level level) {
    for(Vec3 base:List.of(Vec3.ZERO,new Vec3(1.0/8192,-1.0/8192,4.00001),new Vec3(-30000000.125,0,29999999.9))) {
      var codec=new VecDeltaCodec();codec.setBase(base);
      for(Vec3 offset:List.of(Vec3.ZERO,new Vec3(1.0/8192,-1.0/8192,0),new Vec3(7.999755859375,-8,0),new Vec3(8,-8.000244140625,0),new Vec3(0.00000001,1,-0.0001))) {
        Vec3 pos=base.add(offset);var encoded=codec.tryEncode(pos);
        Vec3 decoded=encoded==null?base:encoded.decode(codec).endPosition();
        System.out.println("delta-codec\t"+vec(base)+"\t"+vec(pos)+"\t"+delta(encoded)+"\t"+vec(decoded)+"\t"+vec(new Vec3(VecDeltaCodec.encodingPrecisionLoss(pos.x),VecDeltaCodec.encodingPrecisionLoss(pos.y),VecDeltaCodec.encodingPrecisionLoss(pos.z))));
      }
      var steps=List.of(new PositionStep(base.add(1,0,0),2),new PositionStep(base.add(2,0.125,-1),3));
      System.out.println("delta-steps\t"+vec(base)+"\t"+path(PositionPath.stepped(steps))+"\t"+delta(codec.tryEncode(PositionPath.stepped(steps)))+"\t"+vec(codec.getBase()));
    }
    var entity=new Cow(EntityTypes.COW,level);var tracker=new SteppedInterpolationTracker(entity);
    for(int tick=0;tick<10;tick++) {
      Vec3 pos=new Vec3(tick*0.125,64+tick*0.001,-tick*0.2);entity.setPos(pos);
      entity.syncPosition=tick==1||tick==3||tick==4||tick==7;
      if(tick==4) tracker.applyPredictedMovement(new Vec3(0.1,0.0,0.0));
      tracker.updateTracking(pos);
      String result="-";
      if(tick%3==0) { result=path(tracker.getPositionPath(pos));tracker.clear(); }
      System.out.println("tracked-steps\t"+tick+"\t"+vec(pos)+"\t"+entity.syncPosition+"\t"+result);
    }
  }
}
