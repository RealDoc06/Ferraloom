import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.EntityReference;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.animal.cow.Cow;
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
    var owner=new Cow(EntityTypes.COW,level);
    var reference=EntityReference.of((LivingEntity)owner);
    var changedUuid=new UUID(3,4);owner.setUUID(changedUuid);
    boolean currentUuid=reference.getUUID().equals(changedUuid);
    boolean hidden=reference.getEntity(uuid->null,LivingEntity.class)==owner;
    owner.discard();
    var replacement=new Cow(EntityTypes.COW,level);replacement.setUUID(changedUuid);
    boolean replaced=reference.getEntity(uuid->replacement,LivingEntity.class)==replacement;
    boolean cached=reference.getEntity(uuid->null,LivingEntity.class)==replacement;
    var unresolved=EntityReference.<LivingEntity>of(changedUuid);
    boolean wrongType=unresolved.getEntity(uuid->new PrimedTnt(EntityTypes.TNT,level),LivingEntity.class)==null;
    System.out.println("entity-reference\t"+currentUuid+","+hidden+","+replaced+","+cached+","+wrongType);
    for(long seed:List.of(0L,123L,-1L)) {
      level.getRandom().setSeed(seed);
      var tnt=new PrimedTnt(level,1,64,1,null);
      System.out.println("tnt-launch\t"+seed+"\t"+TrackingVectors.vec(tnt.getDeltaMovement()));
    }
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
