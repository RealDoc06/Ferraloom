import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import java.util.concurrent.CompletableFuture;

/** Executes the real proto-chunk entity stream and section-manager callbacks. */
public final class WorldgenVectors {
  static void visibility(Level level) {
    var events=new ArrayList<String>();
    var callbacks=new LevelCallback<Entity>() {
      void add(String name,Entity entity){events.add(name+":"+entity.getUUID().getLeastSignificantBits());}
      public void onCreated(Entity e){add("Created",e);}
      public void onDestroyed(Entity e){add("Destroyed",e);}
      public void onTickingStart(Entity e){add("TickingStart",e);}
      public void onTickingEnd(Entity e){add("TickingEnd",e);}
      public void onTrackingStart(Entity e){add("TrackingStart",e);}
      public void onTrackingEnd(Entity e){add("TrackingEnd",e);}
      public void onSectionChange(Entity e){add("SectionChange",e);}
    };
    var storage=new EntityPersistentStorage<Entity>() {
      public CompletableFuture<ChunkEntities<Entity>> loadEntities(ChunkPos pos){return new CompletableFuture<>();}
      public void storeEntities(ChunkEntities<Entity> entities){throw new AssertionError();}
      public void flush(boolean durable){throw new AssertionError();}
    };
    var manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,storage);
    for(int index=1;index<=3;index++) {
      Entity entity=index==3?new LivingVectors(level):new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,level);
      entity.setId(index);entity.setUUID(new UUID(0,index));entity.setPos(1.5,64,1.5);manager.addNewEntity(entity);
    }
    events.clear();
    for(var status:List.of(Visibility.TRACKED,Visibility.TICKING,Visibility.TRACKED,Visibility.HIDDEN,Visibility.TICKING)) {
      manager.updateChunkStatus(new ChunkPos(0,0),status);
      var visible=new ArrayList<String>();manager.getEntityGetter().getAll().forEach(e->visible.add(Long.toString(e.getUUID().getLeastSignificantBits())));
      System.out.println("entity-visibility\t"+status+"\t"+String.join(",",events)+"\t"+String.join(",",visible));events.clear();
    }
  }
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
  static void creationOrder(Level level) {
    try {
      var flag=level.getClass().getDeclaredField("captureCreationIds");flag.setAccessible(true);
      flag.setBoolean(level,true);
      Entity[] entities={
        new net.minecraft.world.entity.animal.cow.Cow(EntityTypes.COW,level),
        new net.minecraft.world.entity.item.ItemEntity(EntityTypes.ITEM,level),
        new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,level),
        new net.minecraft.world.entity.item.FallingBlockEntity(EntityTypes.FALLING_BLOCK,level),
        new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level),
        new LivingVectors(level)
      };
      for(Entity entity:entities) System.out.println("entity-creation-order\t"+net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getKey(entity.getType())+"\t"+entity.getId());
      flag.setBoolean(level,false);
    } catch(ReflectiveOperationException error) {throw new AssertionError(error);}
  }
  static String links(List<Entity> roots) {
    var all=new TreeMap<Integer,Entity>();
    var todo=new ArrayDeque<Entity>(roots);
    while(!todo.isEmpty()) {
      var entity=todo.removeFirst();
      if(all.putIfAbsent(entity.getId(),entity)==null)todo.addAll(entity.getPassengers());
    }
    return String.join(",",all.values().stream().map(entity->entity.getId()+":"+entity.getUUID().getLeastSignificantBits()+":"+
      (entity.getVehicle()==null?"none":entity.getVehicle().getId())+":"+
      String.join("/",entity.getPassengers().stream().map(passenger->Integer.toString(passenger.getId())).toList())+":"+entity.isRemoved()).toList());
  }
  static void sharedPublication(Level level) {
    try {
      var flag=level.getClass().getDeclaredField("captureCreationIds");flag.setAccessible(true);flag.setBoolean(level,true);
      var counter=level.getClass().getDeclaredField("creationCounter");counter.setAccessible(true);counter.setInt(level,0);
      var callbacks=new Callbacks();var manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,null);
      load(manager,level,tag("minecraft:tnt",99));
      var anchor=callbacks.created.getFirst();var captured=anchor.getPassengers();
      var first=tag("minecraft:tnt",2);first.put("Passengers",list(tag("minecraft:tnt",3),tag("minecraft:tnt",4)));
      var duplicate=tag("minecraft:tnt",1);duplicate.put("Passengers",list(tag("minecraft:tnt",5)));
      var root=tag("minecraft:tnt",1);root.put("Passengers",list(first,duplicate,tag("missing:unknown",7)));
      load(manager,level,root,tag("minecraft:tnt",6));
      System.out.println("worldgen-shared\tnested\t"+String.join(",",callbacks.events)+"\t"+links(callbacks.created)+"\t"+captured.size());
      counter.setInt(level,0);callbacks=new Callbacks();manager=new PersistentEntitySectionManager<>(Entity.class,callbacks,null);
      root=tag("minecraft:item",1);root.put("Passengers",list(tag("minecraft:tnt",2)));
      load(manager,level,root);
      System.out.println("worldgen-shared\tremoved-vehicle\t"+String.join(",",callbacks.events)+"\t"+links(callbacks.created));
      flag.setBoolean(level,false);
    } catch(ReflectiveOperationException error) {throw new AssertionError(error);}
  }
  public static void run(Level level) {
    visibility(level);
    creationOrder(level);
    sharedPublication(level);
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
