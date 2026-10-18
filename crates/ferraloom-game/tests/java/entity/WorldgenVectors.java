import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.entity.*;
import net.minecraft.world.level.storage.TagValueInput;

/** Executes the real proto-chunk entity stream and section-manager callbacks. */
public final class WorldgenVectors {
  static CompoundTag tag(String type,int uuid) {
    var tag=new CompoundTag();tag.putString("id",type);tag.putIntArray("UUID",new int[]{0,0,0,uuid});
    var pos=new ListTag();pos.add(DoubleTag.valueOf(1.5));pos.add(DoubleTag.valueOf(64));pos.add(DoubleTag.valueOf(1.5));tag.put("Pos",pos);return tag;
  }
  static ListTag list(CompoundTag... tags) {var list=new ListTag();for(var tag:tags)list.add(tag);return list;}
  static final class Callbacks implements LevelCallback<Entity> {
    final List<Entity> created=new ArrayList<>();final List<String> events=new ArrayList<>();
    String id(Entity e) {return Long.toString(e.getUUID().getLeastSignificantBits());}
    @Override public void onCreated(Entity e) {created.add(e);events.add(id(e)+":"+e.getPassengers().size()+":"+e.isPassenger());}
    @Override public void onDestroyed(Entity e) {throw new AssertionError();}
    @Override public void onTickingStart(Entity e) {throw new AssertionError();}
    @Override public void onTickingEnd(Entity e) {throw new AssertionError();}
    @Override public void onTrackingStart(Entity e) {throw new AssertionError();}
    @Override public void onTrackingEnd(Entity e) {throw new AssertionError();}
    @Override public void onSectionChange(Entity e) {throw new AssertionError();}
  }
  static void load(PersistentEntitySectionManager<Entity> manager,Level level,CompoundTag... tags) {
    manager.addWorldGenChunkEntities(EntityType.loadEntitiesRecursive(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),Arrays.asList(tags)),level,EntitySpawnReason.LOAD));
  }
  public static void run(Level level) {
    var callbacks=new Callbacks();var manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,null);
    var root=tag("minecraft:tnt",1);root.put("Passengers",list(tag(":tnt",2),tag("missing:unknown",3)));
    load(manager,level,root,tag("minecraft:tnt",4));
    var links=new ArrayList<String>();
    for(var entity:callbacks.created) links.add(callbacks.id(entity)+":"+entity.getPassengers().size()+":"+(entity.getVehicle()==null?"none":callbacks.id(entity.getVehicle())));
    System.out.println("worldgen-entities\tpreorder\t"+String.join(",",callbacks.events)+"\t"+String.join(",",links)+"\t"+manager.areEntitiesLoaded(new ChunkPos(0,0).pack()));
    callbacks=new Callbacks();manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,null);
    load(manager,level,tag("minecraft:tnt",1));
    root=tag("minecraft:tnt",1);root.put("Passengers",list(tag("minecraft:tnt",2)));load(manager,level,root);
    var child=callbacks.created.get(1);
    System.out.println("worldgen-entities\tduplicate\t"+String.join(",",callbacks.events)+"\t"+callbacks.id(child.getVehicle())+":"+(child.getVehicle()!=callbacks.created.getFirst()));
    callbacks=new Callbacks();manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,null);
    root=tag("missing:unknown",1);root.put("Passengers",list(tag("minecraft:tnt",2)));load(manager,level,root);
    System.out.println("worldgen-entities\tunknown-root\t"+callbacks.created.size());
  }
}
