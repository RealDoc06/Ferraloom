import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.network.syncher.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.entity.monster.cubemob.*;
import net.minecraft.world.level.storage.*;

/** Real cube size tails, original-argument overflow and nested metadata calls. */
public final class CubeVectors {
  static final class ReentrantSlime extends Slime {
    boolean recurse;
    ReentrantSlime(MonsterVectors.Fixture level){super(EntityTypes.SLIME,level);}
    @Override public void onSyncedDataUpdated(EntityDataAccessor<?> key){
      super.onSyncedDataUpdated(key);
      if(recurse && key.equals(ID_SIZE)){recurse=false;setSize(2,false);}
    }
  }
  static final class ReentrantMagma extends MagmaCube {
    boolean recurse;
    ReentrantMagma(MonsterVectors.Fixture level){super(EntityTypes.MAGMA_CUBE,level);}
    @Override public void onSyncedDataUpdated(EntityDataAccessor<?> key){
      super.onSyncedDataUpdated(key);
      if(recurse && key.equals(ID_SIZE)){recurse=false;setSize(2,false);}
    }
  }
  static String state(AbstractCubeMob actor)throws Exception {
    return actor.getSize()+"\t"+MonsterVectors.base(actor,Attributes.MAX_HEALTH)+"\t"+
      MonsterVectors.base(actor,Attributes.MOVEMENT_SPEED)+"\t"+MonsterVectors.base(actor,Attributes.ATTACK_DAMAGE)+"\t"+
      MonsterVectors.base(actor,Attributes.ARMOR)+"\t"+MonsterVectors.fbits(actor.getHealth())+"\t"+MonsterVectors.xp(actor);
  }
  static CompoundTag save(AbstractCubeMob actor,MonsterVectors.Fixture level){
    var output=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,level.registryAccess());
    actor.saveWithoutId(output);return output.buildResult();
  }
  static void capture(MonsterVectors.Fixture level)throws Exception {
    for(var type:List.of(EntityTypes.SLIME,EntityTypes.MAGMA_CUBE)) {
      for(int size:new int[]{-10,0,1,2,127,128,Integer.MIN_VALUE,Integer.MAX_VALUE})for(boolean heal:new boolean[]{false,true}){
        var actor=(AbstractCubeMob)type.create(level,EntitySpawnReason.LOAD);actor.setSize(size,heal);
        System.out.println("cube-resize\t"+MonsterVectors.id(type)+"\t"+size+"\t"+heal+"\tfalse\t"+state(actor));
      }
      for(int age:new int[]{-24000,0,24000}){
        var actor=(AbstractCubeMob)type.create(level,EntitySpawnReason.LOAD);
        var input=new CompoundTag();input.putInt("Age",age);input.putInt("ForcedAge",123);input.putBoolean("AgeLocked",true);
        actor.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),input));
        var saved=save(actor,level);
        System.out.println("cube-age\t"+MonsterVectors.id(type)+"\t"+age+"\t"+actor.getAge()+"\t"+actor.isBaby()+"\t"+
          saved.contains("Age")+"\t"+saved.contains("ForcedAge")+"\t"+saved.contains("AgeLocked"));
      }
      for(boolean heal:new boolean[]{false,true}){
        AbstractCubeMob actor;
        if(type==EntityTypes.SLIME){var slime=new ReentrantSlime(level);slime.recurse=true;actor=slime;}
        else{var magma=new ReentrantMagma(level);magma.recurse=true;actor=magma;}
        actor.setSize(4,heal);
        System.out.println("cube-resize\t"+MonsterVectors.id(type)+"\t4\t"+heal+"\ttrue\t"+state(actor));
      }
      for(int size:new int[]{-10,-1,0,1,2,126,127,128,Integer.MAX_VALUE})for(boolean override:new boolean[]{false,true}){
        var input=new CompoundTag();input.putInt("Size",size);input.putBoolean("wasOnGround",true);
        if(override)input.put("attributes",TagParser.parseCompoundFully("{values:[{id:'minecraft:max_health',base:81.0d},{id:'minecraft:movement_speed',base:0.777d},{id:'minecraft:attack_damage',base:55.0d}]}").get("values"));
        var actor=(AbstractCubeMob)type.create(level,EntitySpawnReason.LOAD);
        actor.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),input));
        var saved=save(actor,level);
        var again=(AbstractCubeMob)type.create(level,EntitySpawnReason.LOAD);
        again.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),saved));
        var repeated=save(again,level);
        System.out.println("cube-roundtrip\t"+MonsterVectors.id(type)+"\t"+size+"\t"+override+"\t"+state(again)+"\t"+
          repeated.getIntOr("Size",0)+"\t"+repeated.getBooleanOr("wasOnGround",false));
      }
    }
  }
}
