import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.monster.zombie.ZombieVillager;
import net.minecraft.world.level.storage.*;

/** Actual early constructor RNG and the native VillagerData/NBT load branches. */
public final class ZombieVillagerVectors {
  static long seed;
  static final class Receiver extends ZombieVillager {
    Receiver(MonsterVectors.Fixture level){super(EntityTypes.ZOMBIE_VILLAGER,level);}
    @Override protected void defineSynchedData(SynchedEntityData.Builder data){
      this.random.setSeed(seed);super.defineSynchedData(data);
    }
  }
  static String state(ZombieVillager actor){
    var data=actor.getVillagerData();
    return data.type().unwrapKey().orElseThrow().identifier()+"\t"+
      data.profession().unwrapKey().orElseThrow().identifier()+"\t"+data.level()+"\t"+
      actor.getVillagerDataFinalized()+"\t"+actor.isConverting();
  }
  static CompoundTag save(ZombieVillager actor,MonsterVectors.Fixture level){
    var output=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,level.registryAccess());
    actor.saveWithoutId(output);return output.buildResult();
  }
  static void capture(MonsterVectors.Fixture level)throws Exception {
    for(long value:new long[]{0,1,2,3,12345,-1,Long.MIN_VALUE,Long.MAX_VALUE}){
      seed=value;var actor=new Receiver(level);
      System.out.println("villager-constructor\t"+seed+"\t"+state(actor)+"\t"+
        MonsterVectors.fbits(actor.getYRot())+"\t"+actor.getRandom().nextLong());
    }
    for(String input:List.of("{}", "{VillagerData:{}}",
      "{VillagerData:{type:'minecraft:desert',profession:'minecraft:farmer',level:5},Xp:42}",
      "{VillagerData:{type:'no_such_type',profession:'unknown',level:0}}",
      "{VillagerData:{type:3,profession:[],level:'bad'}}", "{VillagerData:3}",
      "{VillagerDataFinalized:true}", "{ConversionTime:0,Xp:-3}",
      "{ConversionTime:300,ConversionPlayer:[I;1,2,3,4],Xp:99}",
      "{ConversionTime:-2,ConversionPlayer:[I;1,2,3,4]}", "{ConversionTime:-1}")){
      seed=0;var actor=new Receiver(level);
      actor.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),TagParser.parseCompoundFully(input)));
      var saved=save(actor,level);
      var again=new Receiver(level);
      again.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),saved));
      var repeated=save(again,level);
      System.out.println("villager-nbt\t"+input+"\t"+state(actor)+"\t"+
        saved.getIntOr("ConversionTime",0)+"\t"+saved.getIntOr("Xp",0)+"\t"+
        (saved.contains("ConversionPlayer")?saved.get("ConversionPlayer").toString():"-")+"\t"+
        repeated.getBooleanOr("VillagerDataFinalized",false)+"\t"+actor.getRandom().nextLong());
    }
  }
}
