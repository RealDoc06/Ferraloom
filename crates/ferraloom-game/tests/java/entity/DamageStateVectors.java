import java.util.*;
import net.minecraft.advancements.predicates.NbtPredicate;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.TagValueInput;

/** Actual Living/Player load, virtual absorption, save and reference UUID reads. */
public final class DamageStateVectors {
  static CompoundTag input(String scenario){
    var tag=new CompoundTag();
    if(scenario.equals("absent"))return tag;
    tag.putFloat("Health",7.5f);tag.putShort("HurtTime",(short)10);tag.putShort("DeathTime",(short)12);
    tag.putFloat("AbsorptionAmount",6.25f);tag.putBoolean("FallFlying",true);
    tag.putIntArray("last_hurt_by_player",new int[]{1,2,3,4});tag.putInt("last_hurt_by_player_memory_time",7);
    tag.putIntArray("last_hurt_by_mob",new int[]{5,6,7,8});tag.putInt("ticks_since_last_hurt_by_mob",Integer.MAX_VALUE);
    switch(scenario){
      case "negative" -> {tag.putFloat("AbsorptionAmount",-5);tag.putShort("HurtTime",(short)-1);tag.putShort("DeathTime",(short)-3);tag.putInt("last_hurt_by_player_memory_time",-1);tag.putInt("ticks_since_last_hurt_by_mob",-1);}
      case "nan" -> tag.putFloat("AbsorptionAmount",Float.NaN);
      case "infinity" -> tag.putFloat("AbsorptionAmount",Float.POSITIVE_INFINITY);
      case "wrong" -> {for(String key:List.of("AbsorptionAmount","DeathTime","HurtTime","last_hurt_by_player_memory_time","ticks_since_last_hurt_by_mob"))tag.putString(key,"bad");tag.putString("last_hurt_by_player","00000000-0000-0000-0000-000000000001");tag.putIntArray("last_hurt_by_mob",new int[]{1,2,3});}
      case "numeric" -> {tag.putLongArray("last_hurt_by_player",new long[]{1L<<32,2,3,4});tag.put("last_hurt_by_mob",PersistenceVectors.doubles(1.5,Double.NaN,Double.POSITIVE_INFINITY,Double.NEGATIVE_INFINITY));}
      case "clamp" -> tag.putFloat("Health",50);
    }
    return tag;
  }
  static String ref(CompoundTag tag,String key){return tag.contains(key)?Arrays.toString(tag.getIntArray(key).orElseThrow()).replace(" ",""):"-";}
  static String fields(LivingEntity living,CompoundTag saved){
    return ExperienceVectors.f(living.getHealth())+"\t"+living.hurtTime+"\t"+living.deathTime+"\t"+ExperienceVectors.f(living.getAbsorptionAmount())+"\t"+living.isFallFlying()+"\t"+living.getLastHurtByMobTimestamp()+"\t"+ref(saved,"last_hurt_by_player")+"\t"+saved.getIntOr("last_hurt_by_player_memory_time",-999)+"\t"+ref(saved,"last_hurt_by_mob")+"\t"+saved.getIntOr("ticks_since_last_hurt_by_mob",-999)+"\t"+ExperienceVectors.f(saved.getFloatOr("AbsorptionAmount",-999))+"\t"+saved.getShortOr("DeathTime",(short)-999);
  }
  public static void run(Level level)throws Exception {
    for(String kind:List.of("player","cow"))for(String scenario:List.of("absent","normal","negative","nan","infinity","wrong","numeric","clamp")){
      LivingEntity living=kind.equals("player")?new LivingVectors(level):new Cow(EntityTypes.COW,level);
      living.tickCount=23;
      living.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),input(scenario)));
      System.out.println("hurt-memory-load\t"+kind+"\t"+scenario+"\t"+fields(living,NbtPredicate.getEntityTagToCompare(living)));
    }
    for(String kind:List.of("player","cow"))for(float maximum:new float[]{0,5})for(float amount:new float[]{-1,-0.0f,3,7,Float.NaN,Float.POSITIVE_INFINITY}){
      LivingEntity living=kind.equals("player")?new LivingVectors(level):new Cow(EntityTypes.COW,level);
      living.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(maximum);living.setAbsorptionAmount(amount);
      System.out.println("hurt-absorption\t"+kind+"\t"+ExperienceVectors.f(maximum)+"\t"+ExperienceVectors.f(amount)+"\t"+ExperienceVectors.f(living.getAbsorptionAmount())+"\t"+ExperienceVectors.f(NbtPredicate.getEntityTagToCompare(living).getFloatOr("AbsorptionAmount",-999)));
    }
    var receiver=new LivingVectors(level);receiver.tickCount=17;
    var other=new LivingVectors(level);other.setUUID(new UUID(1,2));receiver.setLastHurtByMob(other);receiver.setLastHurtByPlayer(other,19);
    for(String step:List.of("captured","uuid-changed","removed","cache-invalidated","cleared")){
      if(step.equals("uuid-changed"))other.setUUID(new UUID(3,4));
      if(step.equals("removed"))other.discard();
      if(step.equals("cache-invalidated")){((EntityReference<LivingEntity>)ExperienceVectors.field(LivingEntity.class,"lastHurtByMob").get(receiver)).getEntity(uuid->null,LivingEntity.class);((EntityReference<net.minecraft.world.entity.player.Player>)ExperienceVectors.field(LivingEntity.class,"lastHurtByPlayer").get(receiver)).getEntity(uuid->null,net.minecraft.world.entity.player.Player.class);other.setUUID(new UUID(5,6));}
      if(step.equals("cleared")){receiver.setLastHurtByMob(null);receiver.setLastHurtByPlayer((net.minecraft.world.entity.player.Player)null,0);}
      var saved=NbtPredicate.getEntityTagToCompare(receiver);
      System.out.println("hurt-memory-reference\t"+step+"\t"+ref(saved,"last_hurt_by_player")+"\t"+ref(saved,"last_hurt_by_mob")+"\t"+saved.getIntOr("last_hurt_by_player_memory_time",-999)+"\t"+saved.getIntOr("ticks_since_last_hurt_by_mob",-999));
    }
  }
}
