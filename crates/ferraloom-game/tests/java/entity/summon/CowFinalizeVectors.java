import java.util.*;
import java.lang.reflect.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.resources.*;
import net.minecraft.network.syncher.*;

/** The actual Cow -> AgeableMob -> Mob finalizeSpawn, never a model of it. */
public final class CowFinalizeVectors {
  static final class Fixture extends EntityFixture implements ServerLevelAccessor {
    final Holder<Biome> biome;
    Fixture(RegistryAccess r,String key){super(r);biome=r.lookupOrThrow(Registries.BIOME).getOrThrow(ResourceKey.create(Registries.BIOME,Identifier.parse(key)));}
    @Override public Holder<Biome> getBiome(BlockPos p){return biome;}
    @Override public net.minecraft.world.DifficultyInstance getCurrentDifficultyAt(BlockPos p){throw new AssertionError("unused difficulty service");}
    @Override public ServerLevel getLevel(){throw new AssertionError("unused ServerLevel service");}
  }
  @SuppressWarnings("unchecked")
  static void run(RegistryAccess registries)throws Exception {
    var flags=(EntityDataAccessor<Byte>)ExperienceVectors.field(Mob.class,"DATA_MOB_FLAGS_ID").get(null);
    for(String biome:List.of("minecraft:plains","minecraft:desert","minecraft:snowy_plains"))
      for(long seed:List.of(0L,1L,7L,12345L,987654321L))
        for(String scenario:List.of("new","group-first","group-next","force-baby","never-baby","existing-bonus","already-baby","locked-age")) {
          var level=new Fixture(registries,biome);var cow=new Cow(EntityTypes.COW,level);cow.setPos(8.5,64,8.5);
          cow.setNoAi(true);cow.setAggressive(true);cow.setLeftHanded(true);
          AgeableMob.AgeableMobGroupData group=null;
          if(scenario.equals("group-first"))group=new AgeableMob.AgeableMobGroupData(true);
          if(scenario.equals("group-next")) {group=new AgeableMob.AgeableMobGroupData(true);group.increaseGroupSizeByOne();}
          if(scenario.equals("force-baby")||scenario.equals("already-baby")||scenario.equals("locked-age")) {
            group=new AgeableMob.AgeableMobGroupData(true,1);group.increaseGroupSizeByOne();
          }
          if(scenario.equals("never-baby")){group=new AgeableMob.AgeableMobGroupData(false,1);group.increaseGroupSizeByOne();}
          if(scenario.equals("already-baby"))cow.setAge(-17);
          if(scenario.equals("locked-age")){var method=AgeableMob.class.getDeclaredMethod("setAgeLocked",boolean.class);method.setAccessible(true);method.invoke(cow,true);}
          if(scenario.equals("existing-bonus"))cow.getAttribute(Attributes.FOLLOW_RANGE).addPermanentModifier(new AttributeModifier(Identifier.parse("minecraft:random_spawn_bonus"),0.25,AttributeModifier.Operation.ADD_VALUE));
          cow.getRandom().setSeed(54321);level.getRandom().setSeed(seed);
          var returned=(AgeableMob.AgeableMobGroupData)cow.finalizeSpawn(level,null,EntitySpawnReason.COMMAND,group);
          var modifier=cow.getAttribute(Attributes.FOLLOW_RANGE).getModifier(Identifier.parse("minecraft:random_spawn_bonus"));
          var sound=ExperienceVectors.field(Cow.class,"DATA_SOUND_VARIANT_ID");
          var soundHolder=(Holder<?>)cow.getEntityData().get((EntityDataAccessor<Holder<?>>)sound.get(null));
          var dimensions=cow.getDimensions(cow.getPose());
          System.out.println("cow-finalize\t"+biome+"\t"+seed+"\t"+scenario+"\t"+cow.getVariant().getRegisteredName()+"\t"+soundHolder.getRegisteredName()+"\t"+
            cow.getAge()+"\t"+cow.isBaby()+"\t"+cow.getEntityData().get(flags)+"\t"+returned.getGroupSize()+"\t"+returned.isShouldSpawnBaby()+"\t"+TravelVectors.f(returned.getBabySpawnChance())+"\t"+
            Long.toHexString(Double.doubleToRawLongBits(modifier.amount()))+"\t"+modifier.operation().id()+"\t"+
            cow.getAttribute(Attributes.FOLLOW_RANGE).getPermanentModifiers().contains(modifier)+"\t"+TravelVectors.f(dimensions.width())+","+TravelVectors.f(dimensions.height())+"\t"+
            level.getRandom().nextLong()+"\t"+cow.getRandom().nextLong());
        }
  }
}
