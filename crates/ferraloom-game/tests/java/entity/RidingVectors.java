import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Exercises the real LivingEntity rideTick while a derived tick changes mounts. */
public final class RidingVectors extends Cow {
  Entity next;
  RidingVectors(Level level,Entity next) {super(EntityTypes.COW,level);this.next=next;}
  @Override public void tick() {if(next==null)stopRiding();else startRiding(next,true,false);}
  public static void run(Level level) {
    for(boolean change:java.util.List.of(false,true)) {
      var old=new PrimedTnt(EntityTypes.TNT,level);old.setId(1);old.setPos(1,64,1);
      var next=new PrimedTnt(EntityTypes.TNT,level);next.setId(2);next.setPos(5,67,8);
      var rider=new RidingVectors(level,change?next:null);rider.setId(3);rider.setPos(9,70,9);rider.startRiding(old,true,false);rider.setDeltaMovement(1,2,3);rider.fallDistance=8;
      rider.rideTick();
      System.out.println("riding\t"+(change?"switch":"dismount")+"\t"+TrackingVectors.vec(rider.position())+"\t"+TrackingVectors.vec(rider.getDeltaMovement())+"\t"+Long.toHexString(Double.doubleToRawLongBits(rider.fallDistance))+"\t"+(rider.getVehicle()==next));
    }
  }
}
