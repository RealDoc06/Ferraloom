import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.network.syncher.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.util.RandomSource;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.entity.monster.cubemob.AbstractCubeMob;
import net.minecraft.world.entity.monster.zombie.ZombieVillager;
import net.minecraft.world.level.storage.TagValueInput;

/** Actual native constructors, suppliers, metadata declarations and ordered NBT reads. */
public final class MonsterVectors {
  static List<EntityType<? extends Mob>> types(){return List.of(
    EntityTypes.ZOMBIE,EntityTypes.HUSK,EntityTypes.DROWNED,EntityTypes.ZOMBIE_VILLAGER,
    EntityTypes.ZOMBIFIED_PIGLIN,EntityTypes.SPIDER,EntityTypes.CAVE_SPIDER,
    EntityTypes.SLIME,EntityTypes.MAGMA_CUBE);}
  static final class Fixture extends EntityFixture {
    Fixture(RegistryAccess registry){super(registry);}
    @Override public Difficulty getDifficulty(){return Difficulty.NORMAL;}
    @Override public int getSeaLevel(){return 63;}
  }
  static String bits(double value){return Long.toHexString(Double.doubleToRawLongBits(value));}
  static String fbits(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
  static int xp(Mob mob)throws Exception {
    var field=Mob.class.getDeclaredField("xpReward");field.setAccessible(true);return field.getInt(mob);
  }
  static String id(EntityType<?> type){return BuiltInRegistries.ENTITY_TYPE.getKey(type).toString();}
  static void attributes(EntityType<? extends LivingEntity> type) {
    var supplier=DefaultAttributes.getSupplier(type);
    var values=new TreeMap<String,String>();
    for(var holder:BuiltInRegistries.ATTRIBUTE.listElements().toList())
      if(supplier.hasAttribute(holder))values.put(holder.getRegisteredName(),bits(supplier.getBaseValue(holder)));
    values.forEach((attribute,value)->System.out.println("monster-attribute\t"+id(type)+"\t"+attribute+"\t"+value));
  }
  @SuppressWarnings({"rawtypes","unchecked"})
  static void metadata(Mob mob)throws Exception {
    var declarations=new TreeMap<Integer,String>();
    for(Class<?> cls=mob.getClass();cls!=Mob.class;cls=cls.getSuperclass()) {
      for(var field:cls.getDeclaredFields()) {
        if(!Modifier.isStatic(field.getModifiers()) || field.getType()!=EntityDataAccessor.class)continue;
        field.setAccessible(true);var accessor=(EntityDataAccessor)field.get(null);
        Object value=mob.getEntityData().get(accessor);
        // ZombieVillager's constructor chooses a random profession. Capture that
        // actual method separately with a supplied native RandomSource below.
        String initial=value instanceof Boolean || value instanceof Number?value.toString():"runtime";
        declarations.put(accessor.id(),cls.getSimpleName()+"\t"+field.getName()+"\t"+EntityDataSerializers.getSerializedId(accessor.serializer())+"\t"+initial);
      }
    }
    declarations.forEach((index,value)->System.out.println("monster-data\t"+id(mob.getType())+"\t"+index+"\t"+value));
  }
  static void constructor(Fixture level,EntityType<? extends Mob> type)throws Exception {
    Mob mob=type.create(level,EntitySpawnReason.LOAD);
    if(mob==null)throw new AssertionError(id(type));
    var size=mob.getDimensions(Pose.STANDING);
    System.out.println("monster-constructor\t"+id(type)+"\t"+mob.getClass().getSuperclass().getSimpleName()+"\t"+
      mob.getNavigation().getClass().getSimpleName()+"\t"+mob.getMoveControl().getClass().getSimpleName()+"\t"+
      xp(mob)+"\t"+fbits(size.width())+"\t"+fbits(size.height())+"\t"+fbits(mob.getEyeHeight())+"\t"+fbits(mob.getHealth()));
    attributes(type);metadata(mob);
  }
  static String base(Mob mob,Holder<Attribute> attribute) {
    return mob.getAttributes().hasAttribute(attribute)?bits(mob.getAttributeBaseValue(attribute)):"-";
  }
  static void cube(Fixture level,EntityType<? extends Mob> type)throws Exception {
    for(int size:new int[]{-10,-1,0,1,2,126,127,128,Integer.MAX_VALUE})for(boolean override:new boolean[]{false,true}) {
      var mob=(AbstractCubeMob)type.create(level,EntitySpawnReason.LOAD);
      var tag=new CompoundTag();tag.putInt("Size",size);tag.putBoolean("wasOnGround",true);
      if(override)tag.put("attributes",TagParser.parseCompoundFully("{values:[{id:'minecraft:max_health',base:81.0d},{id:'minecraft:movement_speed',base:0.777d},{id:'minecraft:attack_damage',base:55.0d}]}").get("values"));
      mob.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      var ground=AbstractCubeMob.class.getDeclaredField("wasOnGround");ground.setAccessible(true);
      System.out.println("cube-load\t"+id(type)+"\t"+size+"\t"+override+"\t"+mob.getSize()+"\t"+
        base(mob,Attributes.MAX_HEALTH)+"\t"+base(mob,Attributes.MOVEMENT_SPEED)+"\t"+
        base(mob,Attributes.ATTACK_DAMAGE)+"\t"+base(mob,Attributes.ARMOR)+"\t"+fbits(mob.getHealth())+"\t"+xp(mob)+"\t"+ground.getBoolean(mob));
    }
  }
  static void villagerRandom()throws Exception {
    var method=ZombieVillager.class.getDeclaredMethod("initializeZombieVillagerData",RandomSource.class);
    method.setAccessible(true);
    for(long seed:new long[]{0,1,2,3,12345,-1,Long.MIN_VALUE,Long.MAX_VALUE}) {
      var random=RandomSource.create(seed);
      var data=(net.minecraft.world.entity.npc.villager.VillagerData)method.invoke(null,random);
      System.out.println("zombie-villager-random\t"+seed+"\t"+data.type().unwrapKey().orElseThrow().identifier()+"\t"+
        data.profession().unwrapKey().orElseThrow().identifier()+"\t"+data.level()+"\t"+random.nextLong());
    }
  }

  static void zombieNbt(Fixture level, EntityType<? extends Mob> type)throws Exception {
    for(boolean baby:new boolean[]{false,true})for(int time:new int[]{-2,-1,0,300,Integer.MAX_VALUE}) {
      var mob=(net.minecraft.world.entity.monster.zombie.Zombie)type.create(level,EntitySpawnReason.LOAD);
      var input=new CompoundTag();input.putBoolean("IsBaby",baby);input.putInt("InWaterTime",37);
      input.putInt("DrownedConversionTime",time);
      mob.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),input));
      var output=net.minecraft.world.level.storage.TagValueOutput.createWithContext(ProblemReporter.DISCARDING,level.registryAccess());
      mob.saveWithoutId(output);var saved=output.buildResult();
      var again=(net.minecraft.world.entity.monster.zombie.Zombie)type.create(level,EntitySpawnReason.LOAD);
      again.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),saved));
      var repeat=net.minecraft.world.level.storage.TagValueOutput.createWithContext(ProblemReporter.DISCARDING,level.registryAccess());
      again.saveWithoutId(repeat);var repeated=repeat.buildResult();
      for(var key:List.of("IsBaby","CanBreakDoors","InWaterTime","DrownedConversionTime"))
        if(!Objects.equals(saved.get(key),repeated.get(key)))throw new AssertionError(key);
      System.out.println("zombie-nbt\t"+id(type)+"\t"+baby+"\t"+time+"\t"+
        saved.getBooleanOr("IsBaby",false)+"\t"+saved.getBooleanOr("CanBreakDoors",false)+"\t"+
        saved.getIntOr("InWaterTime",0)+"\t"+saved.getIntOr("DrownedConversionTime",0)+"\t"+
        bits(mob.getAttributeValue(Attributes.MOVEMENT_SPEED))+"\t"+mob.isUnderWaterConverting());
    }
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;var errors=System.err;
    SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
    var registries=NeighborVectors.load();
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    var level=new Fixture(registries);
    for(var type:types())constructor(level,type);
    ZombieSubclassVectors.capture(level);ZombieVillagerVectors.capture(level);
    zombieNbt(level,EntityTypes.DROWNED);zombieNbt(level,EntityTypes.ZOMBIE);zombieNbt(level,EntityTypes.HUSK);
    cube(level,EntityTypes.SLIME);cube(level,EntityTypes.MAGMA_CUBE);villagerRandom();
  }
}
