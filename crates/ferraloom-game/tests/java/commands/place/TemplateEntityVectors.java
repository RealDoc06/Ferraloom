import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real StructureTemplate#placeEntities; its EntityType#create receiver is controlled. */
public class TemplateEntityVectors {
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var method=StructureTemplate.class.getDeclaredMethod("placeEntities",net.minecraft.world.level.ServerLevelAccessor.class,BlockPos.class,Mirror.class,Rotation.class,BlockPos.class,BoundingBox.class,boolean.class,ProblemReporter.class);method.setAccessible(true);
  var entitiesField=StructureTemplate.class.getDeclaredField("entityInfoList");entitiesField.setAccessible(true);
  var inputField=net.minecraft.world.level.storage.TagValueInput.class.getDeclaredField("input");inputField.setAccessible(true);
  var rows=new JsonArray();
  for(float yaw:List.of(35.5f,-140.25f))for(var rotation:Rotation.values())for(var mirror:Mirror.values()) {
   var row=new JsonObject();row.addProperty("rotation",rotation.name());row.addProperty("mirror",mirror.name());
   var template=new StructureTemplate();var tag=TagParser.parseCompoundFully("{id:'minecraft:tnt',UUID:[I;0,0,0,123],Rotation:["+yaw+"f,-12.5f],Passengers:[{id:'minecraft:zombie'}]}");
   ((List<StructureTemplate.StructureEntityInfo>)entitiesField.get(template)).add(new StructureTemplate.StructureEntityInfo(new Vec3(1.25,2.5,3.75),new BlockPos(1,2,3),tag));
   var level=mock(ServerLevel.class);doReturn(level).when(level).getLevel();doReturn(RegistryAccess.EMPTY).when(level).registryAccess();
   var entity=mock(Entity.class);doReturn(yaw).when(entity).getYRot();doReturn(-12.5f).when(entity).getXRot();doCallRealMethod().when(entity).rotate(any());doCallRealMethod().when(entity).mirror(any());
   doAnswer(i->{var position=new JsonArray();for(int n=0;n<3;n++)position.add((double)i.getArgument(n));row.add("position",position);row.addProperty("yaw_bits",Integer.toUnsignedLong(Float.floatToRawIntBits(i.getArgument(3))));row.addProperty("pitch_bits",Integer.toUnsignedLong(Float.floatToRawIntBits(i.getArgument(4))));return null;}).when(entity).snapTo(anyDouble(),anyDouble(),anyDouble(),anyFloat(),anyFloat());
   try(var factory=mockStatic(EntityType.class,CALLS_REAL_METHODS)) {
    factory.when(()->EntityType.create(any(ValueInput.class),eq(level),any(EntitySpawnRequest.class))).thenAnswer(i->{row.addProperty("tag",inputField.get(i.getArgument(0)).toString());var request=(EntitySpawnRequest)i.getArgument(2);row.addProperty("reason",request.reason().name());row.addProperty("ignore_checks",request.ignoreChecks());return Optional.of(entity);});
    method.invoke(template,level,new BlockPos(8,80,8),mirror,rotation,BlockPos.ZERO,null,false,ProblemReporter.DISCARDING);
   }
   verify(level).addFreshEntityWithPassengers(entity);rows.add(row);
  }
  Files.writeString(Path.of(args[0]),PlaceVectors.JSON.toJson(rows)+"\n");System.out.println("Captured "+rows.size()+" native template entity callbacks");
 }
}
