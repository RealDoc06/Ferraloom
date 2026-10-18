import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.TagValueInput;

/** Exercises Entity.load's rotation setters and whole-vector motion validation. */
public final class PersistenceVectors {
  static ListTag floats(float... values) {
    var list=new ListTag();for(float value:values) list.add(FloatTag.valueOf(value));return list;
  }
  static ListTag doubles(double... values) {
    var list=new ListTag();for(double value:values) list.add(DoubleTag.valueOf(value));return list;
  }
  static String rotation(PrimedTnt entity) {
    return Integer.toHexString(Float.floatToRawIntBits(entity.getYRot()))+","+Integer.toHexString(Float.floatToRawIntBits(entity.getXRot()))+","+Integer.toHexString(Float.floatToRawIntBits(entity.yRotO))+","+Integer.toHexString(Float.floatToRawIntBits(entity.xRotO));
  }
  public static void run(Level level) {
    float[][] rotations={{-721.25f,-721.25f},{721.25f,180},{-0.0f,-0.0f},{Float.NaN,Float.POSITIVE_INFINITY},{37.25f,-450.125f}};
    for(int i=0;i<rotations.length;i++) {
      var entity=new PrimedTnt(EntityTypes.TNT,level);entity.setYRot(11.25f);entity.setXRot(-12.5f);entity.setDeltaMovement(1,2,3);
      var tag=new CompoundTag();tag.put("Rotation",floats(rotations[i]));tag.put("Motion",doubles(0.5,0.25,-0.25));
      entity.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      System.out.println("nbt-rotation\t"+i+"\t"+rotation(entity)+"\t"+TrackingVectors.vec(entity.getDeltaMovement()));
    }
    double[][] motion={{Double.NaN,8,9},{10,-10.00001,Double.POSITIVE_INFINITY},{Double.NEGATIVE_INFINITY,-0.0,0.25}};
    for(int i=0;i<motion.length;i++) {
      var entity=new PrimedTnt(EntityTypes.TNT,level);entity.setDeltaMovement(1,2,3);
      var tag=new CompoundTag();tag.put("Motion",doubles(motion[i]));
      entity.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      System.out.println("nbt-motion\t"+i+"\t"+TrackingVectors.vec(entity.getDeltaMovement()));
    }
  }
}
