import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.storage.*;
import net.minecraft.util.ProblemReporter;

/** EntityType#create alone, plus the actual Entity transform virtuals. */
public final class SingleRootVectors {
 public static void main(String[] args)throws Exception {
  var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
  var registries=NeighborVectors.load();
  var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
  var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
  BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
  for(String scenario:List.of("plain","passenger","unported-passenger","unknown","player","invalid-position")) {
   var level=new SummonVectors.Fixture(registries);level.captureCreationIds=true;
   var tag=WorldgenVectors.tag(scenario.equals("unknown")?"missing:unknown":scenario.equals("player")?"minecraft:player":"minecraft:tnt",42);
   if(scenario.equals("passenger")||scenario.equals("unported-passenger"))tag.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag(scenario.equals("passenger")?"minecraft:tnt":"minecraft:pig",43)));
   if(scenario.equals("invalid-position")){var pos=new ListTag();pos.add(DoubleTag.valueOf(Double.NaN));pos.add(DoubleTag.valueOf(64));pos.add(DoubleTag.valueOf(0));tag.put("Pos",pos);}
   String result;try {var entity=EntityType.create(TagValueInput.create(ProblemReporter.DISCARDING,context,tag),level,new EntitySpawnRequest(EntitySpawnReason.COMMAND,false)).orElse(null);result=entity==null?"none":entity.typeHolder().getRegisteredName()+":"+entity.getUUID().getLeastSignificantBits()+":"+entity.getPassengers().size()+":"+entity.isPassenger();}catch(RuntimeException exception){result="error";}
   System.out.println("entity-single\t"+scenario+"\t"+tag+"\t"+result+"\t"+level.creationCounter);
  }
  var level=new SummonVectors.Fixture(registries);var entity=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,level);
  for(float angle:new float[]{0f,-0f,1f,-1f,180f,-180f,270f,-270f,360f,-360f,720.25f,-720.25f,Float.MAX_VALUE,-Float.MAX_VALUE,Float.POSITIVE_INFINITY,Float.NEGATIVE_INFINITY,Float.NaN}) {
   // Bypass setYRot's invalid-angle rejection to measure the virtual itself.
   var yaw=Entity.class.getDeclaredField("yRot");yaw.setAccessible(true);yaw.setFloat(entity,angle);
   for(var rotation:Rotation.values())System.out.println("entity-rotate\t"+Integer.toHexString(Float.floatToIntBits(angle))+"\t"+rotation+"\t"+Integer.toHexString(Float.floatToIntBits(entity.rotate(rotation)))+"\t"+Integer.toHexString(Float.floatToIntBits(entity.getYRot())));
   for(var mirror:Mirror.values())System.out.println("entity-mirror\t"+Integer.toHexString(Float.floatToIntBits(angle))+"\t"+mirror+"\t"+Integer.toHexString(Float.floatToIntBits(entity.mirror(mirror)))+"\t"+Integer.toHexString(Float.floatToIntBits(entity.getYRot())));
  }
 }
}
