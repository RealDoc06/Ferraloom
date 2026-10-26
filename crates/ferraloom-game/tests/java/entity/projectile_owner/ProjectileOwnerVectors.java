import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.entity.projectile.throwableitemprojectile.Snowball;
import net.minecraft.world.level.storage.*;
import static org.mockito.Mockito.*;

/** Actual Projectile owner cache, effect source and subclass persistence. */
public final class ProjectileOwnerVectors {
 static final class Probe extends Projectile {
  Probe(net.minecraft.world.level.Level level){super(EntityTypes.SNOWBALL,level);}
  @Override protected void defineSynchedData(net.minecraft.network.syncher.SynchedEntityData.Builder builder){}
 }
 public static void main(String[] args)throws Exception {
  var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
  var registries=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
  var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
  BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(p->p.apply());
  var level=mock(ServerLevel.class);when(level.registryAccess()).thenReturn(registries);
  var input=Projectile.class.getDeclaredMethod("readAdditionalSaveData",ValueInput.class);input.setAccessible(true);
  var output=Projectile.class.getDeclaredMethod("addAdditionalSaveData",ValueOutput.class);output.setAccessible(true);
  for(String text:List.of("{}","{Owner:[I;1,2,3,4]}","{Owner:[I;1,2,3]}","{Owner:'bad',LeftOwner:1b,HasBeenShot:1b}","{LeftOwner:'bad',HasBeenShot:'bad'}","{LeftOwner:0b,HasBeenShot:0b}","{LeftOwner:1b,HasBeenShot:1b}","{can_break:{blocks:'minecraft:stone'}}","{can_break:[]}")) {
   var entity=new Probe(level);var tag=TagParser.parseCompoundFully(text);
   input.invoke(entity,TagValueInput.create(ProblemReporter.DISCARDING,context,tag));
   var saved=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,context);output.invoke(entity,saved);
   System.out.println("projectile-nbt\t"+tag+"\t"+saved.buildResult());
  }
  var lookup=new HashMap<UUID,Entity>();var calls=new AtomicInteger();
  when(level.getEntityInAnyDimension(any(UUID.class))).thenAnswer(call->{calls.incrementAndGet();return lookup.get(call.getArgument(0));});
  var projectile=new Probe(level);var owner=new PrimedTnt(EntityTypes.TNT,level);owner.setUUID(new UUID(1,2));
  var field=Projectile.class.getDeclaredField("owner");field.setAccessible(true);
  for(int step=0;step<7;step++) {
   switch(step) {
    case 0->projectile.setOwner(owner);
    case 1->owner.setUUID(new UUID(3,4));
    case 2->owner.discard();
    case 3->{var replacement=new PrimedTnt(EntityTypes.TNT,level);replacement.setUUID(new UUID(3,4));lookup.put(replacement.getUUID(),replacement);}
    case 4->lookup.clear();
    case 5->projectile.setOwner((Entity)null);
    case 6->{var tag=TagParser.parseCompoundFully("{Owner:[I;0,3,0,4]}");input.invoke(projectile,TagValueInput.create(ProblemReporter.DISCARDING,context,tag));}
   }
   var got=projectile.getOwner();var effect=projectile.getEffectSource();var reference=(EntityReference<?>)field.get(projectile);
   System.out.println("projectile-owner\t"+step+"\t"+(got==null?"none":got==owner?"original":"replacement")+"\t"+(reference==null?"none":reference.getUUID())+"\t"+(effect==projectile?"self":effect==owner?"original":"replacement")+"\t"+calls.get());
  }
 }
}
