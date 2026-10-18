import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/** Runs actual WorldBorder extents, bound queries and collision shapes. */
public final class BorderVectors {
  static String doubles(double... values) {
    var result=new java.util.StringJoiner(",");for(double value:values)result.add(Long.toHexString(Double.doubleToRawLongBits(value)));return result.toString();
  }
  static String snapshot(WorldBorder border) {
    return doubles(border.getSize(),border.getMinX(),border.getMaxX(),border.getMinZ(),border.getMaxZ(),border.getMaxX(0.5f),border.getMaxX(1),border.getLerpSpeed(),border.getLerpTarget())+"\t"+border.getLerpTime()+"\t"+border.getStatus();
  }
  public static void run(Level level) {
    double[][] lerps={{100.25,101.75,4,100},{1000.125,7.25,3,Long.MAX_VALUE},{11.5,11.5,3,100},{100,10,0,100},{100,10,-1,100}};
    for(int scenario=0;scenario<lerps.length;scenario++) {
      var values=lerps[scenario];var border=new WorldBorder();border.setCenter(0.25,-0.25);
      long time=scenario==1?Long.MAX_VALUE:(long)values[3];
      border.lerpSizeBetween(values[0],values[1],(long)values[2],time);
      for(int tick=0;tick<=Math.max(1,(int)values[2]+1);tick++) {
        System.out.println("border-lerp\t"+scenario+"\t"+tick+"\t"+snapshot(border));border.tick();
      }
    }
    double[][] geometry={{0,0,10,29999984},{0.25,-0.25,10.5,29999984},{29999980,-29999980,100,29999984},{-0.25,0.25,100,8}};
    for(int scenario=0;scenario<geometry.length;scenario++) {
      var values=geometry[scenario];var border=new WorldBorder();border.setCenter(values[0],values[1]);border.setSize(values[2]);border.setAbsoluteMaxSize((int)values[3]);
      for(int offset=-6;offset<=6;offset+=3) {
        double x=values[0]+offset,z=values[1]-offset;var position=new Vec3(x,70.25,z);var block=BlockPos.containing(position);var box=new AABB(x-0.25,64,z-0.25,x+0.25,65,z+0.25);
        var entity=new PrimedTnt(EntityTypes.TNT,level);entity.setPos(x,64,z);
        String flags=""+(border.isWithinBounds(x,z)?1:0)+(border.isWithinBounds(block)?1:0)+(border.isWithinBounds(ChunkPos.containing(block))?1:0)+(border.isWithinBounds(box)?1:0)+(border.isInsideCloseToBorder(entity,box)?1:0);
        System.out.println("border-geometry\t"+scenario+"\t"+offset+"\t"+flags+"\t"+TrackingVectors.vec(border.clampVec3ToBound(position))+"\t"+doubles(border.getDistanceToBorder(x,z),border.getCollisionShape().collide(Direction.Axis.X,box,4)));
      }
    }
  }
}
