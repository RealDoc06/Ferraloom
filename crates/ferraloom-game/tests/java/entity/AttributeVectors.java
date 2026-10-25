import java.util.*;
import com.google.common.collect.ArrayListMultimap;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.nbt.*;

/** Executes real AttributeInstance arithmetic, callback and persistent codec rules. */
public final class AttributeVectors {
  static AttributeModifier modifier(String id,double value,AttributeModifier.Operation op) {
    return new AttributeModifier(Identifier.withDefaultNamespace(id),value,op);
  }
  static String bits(double value) {return Long.toHexString(Double.doubleToRawLongBits(value));}
  static String ids(Collection<AttributeModifier> values) {return String.join(",",values.stream().map(v->v.id().getPath()).toList());}
  static void emit(String name,AttributeInstance instance,int[] callbacks) {
    System.out.println("attribute\t"+name+"\t"+bits(instance.getBaseValue())+"\t"+bits(instance.getValue())+"\t"+ids(instance.getModifiers())+"\t"+ids(instance.getPermanentModifiers())+"\t"+callbacks[0]);
  }
  public static void run() {
    int[] changed={0};
    var instance=new AttributeInstance(Attributes.MOVEMENT_SPEED,a->changed[0]++);
    emit("default",instance,changed);
    instance.setBaseValue(0.125);emit("base",instance,changed);
    instance.addTransientModifier(modifier("first",0.2,AttributeModifier.Operation.ADD_VALUE));
    instance.addPermanentModifier(modifier("base",0.3,AttributeModifier.Operation.ADD_MULTIPLIED_BASE));
    instance.addPermanentModifier(modifier("total",0.4,AttributeModifier.Operation.ADD_MULTIPLIED_TOTAL));
    emit("three-operations",instance,changed);
    var same=modifier("first",0.5,AttributeModifier.Operation.ADD_MULTIPLIED_TOTAL);
    instance.addOrUpdateTransientModifier(same);emit("changed-operation",instance,changed);
    instance.addOrUpdateTransientModifier(same);emit("same-object",instance,changed);
    instance.addOrUpdateTransientModifier(modifier("first",0.5,AttributeModifier.Operation.ADD_MULTIPLIED_TOTAL));emit("equal-new-object",instance,changed);
    instance.removeModifier(same.id());emit("remove-changed-operation",instance,changed);
    instance.removeModifiers();emit("remove-all-stale-operation",instance,changed);
    for(int i=0;i<40;i++) instance.addPermanentModifier(modifier("entry_"+i,(i%4==0?1e8:i%4==1?-1e8:i/100.0),AttributeModifier.Operation.ADD_VALUE));
    emit("rehash",instance,changed);
    var largeCopy=new AttributeInstance(Attributes.MOVEMENT_SPEED,a->changed[0]++);largeCopy.replaceFrom(instance);emit("replace-large",largeCopy,changed);
    for(int i=0;i<32;i++) instance.removeModifier(Identifier.withDefaultNamespace("entry_"+i));
    emit("shrink",instance,changed);
    var copy=new AttributeInstance(Attributes.MOVEMENT_SPEED,a->changed[0]++);copy.replaceFrom(instance);emit("replace",copy,changed);
    instance.setBaseValue(Double.NaN);emit("nan",instance,changed);
    instance.setBaseValue(Double.POSITIVE_INFINITY);emit("positive-infinity",instance,changed);
    instance.setBaseValue(Double.NEGATIVE_INFINITY);emit("negative-infinity",instance,changed);
    var gravity=new AttributeInstance(Attributes.GRAVITY,a->changed[0]++);gravity.setBaseValue(-0.0);emit("negative-zero",gravity,changed);
    gravity.setBaseValue(0.0);emit("equal-positive-zero",gravity,changed);
    var map=new AttributeMap(DefaultAttributes.getSupplier(EntityTypes.PLAYER));
    System.out.println("attribute-map\tfresh\t"+map.getAttributesToSync().size()+"\t"+map.getAttributesToUpdate().size()+"\t"+map.getSyncableAttributes().size());
    var health=map.getInstance(Attributes.MAX_HEALTH);
    System.out.println("attribute-map\tcreated\t"+map.getAttributesToSync().size()+"\t"+map.getAttributesToUpdate().size()+"\t"+map.getSyncableAttributes().size());
    map.getAttributesToSync().clear();map.getAttributesToUpdate().clear();health.setBaseValue(20);
    System.out.println("attribute-map\tunchanged\t"+map.getAttributesToSync().size()+"\t"+map.getAttributesToUpdate().size()+"\t"+map.getSyncableAttributes().size());
    health.setBaseValue(30);
    System.out.println("attribute-map\tmodified\t"+map.getAttributesToSync().size()+"\t"+map.getAttributesToUpdate().size()+"\t"+map.getSyncableAttributes().size());
    for(String scenario:List.of("absent","created","transient","permanent","different-id","mixed","unsupported")) {
      var removal=new AttributeMap(DefaultAttributes.getSupplier(EntityTypes.PLAYER));
      var attribute=scenario.equals("unsupported")?Attributes.FOLLOW_RANGE:Attributes.SUBMERGED_MINING_SPEED;
      if(!scenario.equals("absent")&&!scenario.equals("unsupported")) {
        var current=removal.getInstance(attribute);
        if(scenario.equals("transient")||scenario.equals("mixed"))current.addTransientModifier(modifier("remove",0.5,AttributeModifier.Operation.ADD_VALUE));
        if(scenario.equals("permanent"))current.addPermanentModifier(modifier("remove",0.5,AttributeModifier.Operation.ADD_VALUE));
        if(scenario.equals("different-id")||scenario.equals("mixed"))current.addPermanentModifier(modifier("keep",0.25,AttributeModifier.Operation.ADD_VALUE));
      }
      removal.getAttributesToSync().clear();removal.getAttributesToUpdate().clear();
      var modifiers=ArrayListMultimap.<Holder<Attribute>,AttributeModifier>create();
      modifiers.put(attribute,modifier("remove",0.5,AttributeModifier.Operation.ADD_VALUE));
      removal.removeAttributeModifiers(modifiers);
      var saved=AttributeInstance.Packed.CODEC.listOf().encodeStart(NbtOps.INSTANCE,removal.pack()).getOrThrow();
      System.out.println("attribute-removal\t"+scenario+"\t"+saved+"\t"+removal.getAttributesToSync().size()+"\t"+removal.getAttributesToUpdate().size());
    }
    String[] tags={"{id:'minecraft:max_health'}","{id:'minecraft:max_health',base:'bad'}","{id:'minecraft:max_health',base:30.0,modifiers:[{id:'minecraft:yes',amount:2.0,operation:'add_value'},{id:'minecraft:no',operation:'add_value'}]}","{id:'minecraft:max_health',modifiers:'bad'}","{id:'minecraft:unknown',base:30.0}"};
    for(int i=0;i<tags.length;i++) try {
      var result=AttributeInstance.Packed.CODEC.parse(NbtOps.INSTANCE,TagParser.parseCompoundFully(tags[i]));
      var value=result.resultOrPartial(error->{});
      System.out.println("attribute-nbt\t"+i+"\t"+result.isSuccess()+"\t"+value.map(p->bits(p.baseValue())+":"+ids(p.modifiers())).orElse("none"));
    } catch(Exception error) {throw new RuntimeException(error);}
  }
}
