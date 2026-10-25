import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.*;
import net.minecraft.world.level.entity.*;

/** Actual recursive loading, post-load callbacks, startRiding and fresh add. */
public final class SummonVectors {
  static final class Fixture extends EntityFixture {
    Difficulty difficulty=Difficulty.NORMAL;
    Fixture(RegistryAccess registries){super(registries);}
    @Override public Difficulty getDifficulty(){return difficulty;}
  }
  static String tree(Entity entity) {
    if(entity==null)return "-";
    return String.join(";",entity.getSelfAndPassengers().map(e->e.getId()+":"+
      e.typeHolder().getRegisteredName()+":"+e.getUUID().getLeastSignificantBits()+":"+
      (e.getVehicle()==null?"-":e.getVehicle().getId())+":"+
      String.join("/",e.getPassengers().stream().map(child->Integer.toString(child.getId())).toList())+":"+
      e.isRemoved()+":"+TrackingVectors.vec(e.position())+":"+TrackingVectors.vec(e.getDeltaMovement())+":"+
      TravelVectors.f(e.getYRot())+","+TravelVectors.f(e.getXRot())).toList());
  }
  static final class Callbacks implements LevelCallback<Entity> {
    final List<String> created=new ArrayList<>();
    @Override public void onCreated(Entity e){created.add(Integer.toString(e.getId()));}
    @Override public void onDestroyed(Entity e){throw new AssertionError();}
    @Override public void onTickingStart(Entity e){throw new AssertionError();}
    @Override public void onTickingEnd(Entity e){throw new AssertionError();}
    @Override public void onTrackingStart(Entity e){throw new AssertionError();}
    @Override public void onTrackingEnd(Entity e){throw new AssertionError();}
    @Override public void onSectionChange(Entity e){throw new AssertionError();}
  }
  static void run(RegistryAccess registries)throws Exception {
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    for(String scenario:List.of("nested","duplicate-root","duplicate-passenger","internal-duplicate",
        "removed-root","unknown-root","unknown-child","invalid-position","player-null","peaceful-zombie")) {
      var level=new Fixture(registries);level.captureCreationIds=true;
      var callbacks=new Callbacks();var manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,null);
      var server=(ServerLevel)allocator.allocateInstance(ServerLevel.class);
      ExperienceVectors.field(ServerLevel.class,"entityManager").set(server,manager);
      var existing=new PrimedTnt(EntityTypes.TNT,level);existing.setUUID(new UUID(0,99));manager.addNewEntity(existing);callbacks.created.clear();
      var root=WorldgenVectors.tag("minecraft:tnt",1);
      root.put("Rotation",WorldgenVectors.list()); // Absent/invalid optional rotation defaults to zero.
      switch(scenario) {
        case "nested" -> {
          var child=WorldgenVectors.tag("minecraft:cow",2);child.putInt("Age",-100);
          child.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag("minecraft:experience_orb",3)));
          var item=WorldgenVectors.tag("minecraft:item",4);var stack=new CompoundTag();stack.putString("id","minecraft:diamond");stack.putInt("count",3);item.put("Item",stack);
          root.put("Passengers",WorldgenVectors.list(child,item));
        }
        case "duplicate-root" -> root.putIntArray("UUID",new int[]{0,0,0,99});
        case "duplicate-passenger" -> root.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag("minecraft:tnt",99)));
        case "internal-duplicate" -> root.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag("minecraft:tnt",1)));
        case "removed-root" -> {root=WorldgenVectors.tag("minecraft:item",1);root.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag("minecraft:tnt",2)));}
        case "unknown-root" -> {root=WorldgenVectors.tag("missing:unknown",1);root.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag("minecraft:tnt",2)));}
        case "unknown-child" -> {
          var unknown=WorldgenVectors.tag("missing:unknown",2);unknown.put("Passengers",WorldgenVectors.list(WorldgenVectors.tag("minecraft:tnt",3)));
          root.put("Passengers",WorldgenVectors.list(unknown,WorldgenVectors.tag("minecraft:tnt",4)));
        }
        case "invalid-position" -> {var pos=new ListTag();pos.add(DoubleTag.valueOf(Double.NaN));pos.add(DoubleTag.valueOf(64));pos.add(DoubleTag.valueOf(0));root.put("Pos",pos);}
        case "player-null" -> root=WorldgenVectors.tag("minecraft:player",1);
        case "peaceful-zombie" -> {root=WorldgenVectors.tag("minecraft:zombie",1);level.difficulty=Difficulty.PEACEFUL;}
      }
      var post=new ArrayList<String>();
      var entity=EntityType.loadEntityRecursive(root,level,new EntitySpawnRequest(EntitySpawnReason.COMMAND,false),e->{
        post.add(e.getId()+":"+e.typeHolder().getRegisteredName()+":"+e.getUUID().getLeastSignificantBits()+":"+
          e.getPassengers().size()+":"+e.isPassenger()+":"+e.isRemoved());
        e.snapTo(8.5,70,8.5,e.getYRot(),e.getXRot());return e;
      });
      String before=tree(entity);String added=entity==null?"none":Boolean.toString(server.tryAddFreshEntityWithPassengers(entity));
      System.out.println("entity-recursive\t"+scenario+"\t"+root+"\t"+level.difficulty.getId()+"\t"+
        (post.isEmpty()?"-":String.join(";",post))+"\t"+before+"\t"+added+"\t"+
        (callbacks.created.isEmpty()?"-":String.join(",",callbacks.created))+"\t"+level.creationCounter);
    }
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
    var registries=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    run(registries);
    VariantVectors.run(registries,context);
  }
}
