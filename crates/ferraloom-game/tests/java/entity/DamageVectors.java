import java.lang.reflect.Field;
import java.util.*;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.tags.DamageTypeTags;
import net.minecraft.tags.TagKey;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.phys.Vec3;

/** Observes the real damage registry, holder tags and DamageType codec. */
public final class DamageVectors {
  static String fields(DamageType type) {
    return type.msgId()+"\t"+type.scaling().getSerializedName()+"\t"+
      Integer.toHexString(Float.floatToRawIntBits(type.exhaustion()))+"\t"+
      type.effects().getSerializedName()+"\t"+type.deathMessageType().getSerializedName();
  }
  public static void run(Level level) throws Exception {
    var registry=level.registryAccess().lookupOrThrow(Registries.DAMAGE_TYPE);
    var tags=new TreeMap<String,TagKey<DamageType>>();
    for(Field field:DamageTypeTags.class.getFields()) {
      @SuppressWarnings("unchecked") var tag=(TagKey<DamageType>)field.get(null);
      tags.put(tag.location().toString(),tag);
    }
    for(var holder:registry.listElements().sorted(Comparator.comparing(h->h.key().identifier().toString())).toList()) {
      var memberships=new ArrayList<String>();
      for(var entry:tags.entrySet())if(holder.is(entry.getValue()))memberships.add(entry.getKey());
      System.out.println("damage-type\t"+holder.key().identifier()+"\t"+fields(holder.value())+"\t"+
        String.join(",",memberships)+"\t"+holder.value().effects().sound().location());
    }
    String base="\"message_id\":\"custom\",\"scaling\":\"never\",\"exhaustion\":0.1";
    String[] cases={"{"+base+"}","{"+base+",\"effects\":\"thorns\",\"death_message_type\":\"fall_variants\"}",
      "{"+base+",\"effects\":\"bad\"}","{"+base+",\"effects\":null}","{"+base+",\"effects\":5}",
      "{"+base+",\"death_message_type\":\"bad\"}","{"+base+",\"death_message_type\":null}",
      "{\"message_id\":\"custom\",\"scaling\":\"always\",\"exhaustion\":\"0.125\"}",
      "{\"message_id\":\"custom\",\"scaling\":\"always\",\"exhaustion\":true}",
      "{\"message_id\":\"custom\",\"scaling\":\"always\",\"exhaustion\":1e100}",
      "{\"message_id\":123,\"scaling\":\"never\",\"exhaustion\":-0.0}",
      "{\"message_id\":true,\"scaling\":\"never\",\"exhaustion\":0}",
      "{\"message_id\":\"\",\"scaling\":\"never\",\"exhaustion\":-0.0}",
      "{\"message_id\":\"custom\",\"scaling\":\"bad\",\"exhaustion\":0}",
      "{\"message_id\":\"custom\",\"exhaustion\":0}","[]","null",
      "{"+base+",\"unknown\":[1,2,3]}",
      "{\"message_id\":\"x\",\"scaling\":\"never\",\"exhaustion\":0.100000000000000005}"};
    for(int i=0;i<cases.length;i++) {
      var result=DamageType.DIRECT_CODEC.parse(JsonOps.INSTANCE,JsonParser.parseString(cases[i]));
      System.out.println("damage-codec\t"+i+"\t"+cases[i]+"\t"+result.isSuccess()+"\t"+
        result.result().map(DamageVectors::fields).orElse("none"));
    }
    var player=new LivingVectors(level);player.getAbilities().instabuild=true;
    var cow=new Cow(EntityTypes.COW,level);var tnt=new PrimedTnt(EntityTypes.TNT,level);
    Entity[] actors={null,player,cow,tnt};
    var holder=registry.getOrThrow(DamageTypes.GENERIC);
    var sources=level.damageSources();
    var names=new ArrayList<String>();var samples=new ArrayList<DamageSource>();
    for(int cause=0;cause<actors.length;cause++) {
      names.add("cause-"+cause);samples.add(new DamageSource(holder,actors[cause]));
    }
    names.add("indirect");samples.add(new DamageSource(holder,tnt,cow));
    names.add("explicit");samples.add(new DamageSource(holder,new Vec3(-0.0,100,3)));
    for(int direct:new int[]{0,3})for(int cause:new int[]{0,2}) {
      names.add("explosion-"+direct+"-"+cause);samples.add(sources.explosion(actors[direct],actors[cause]));
    }
    names.add("player");samples.add(sources.playerAttack(player));
    names.add("mob");samples.add(sources.mobAttack(cow));
    names.add("thorns");samples.add(sources.thorns(tnt));
    names.add("bad-respawn");samples.add(sources.badRespawnPointExplosion(new Vec3(-0.0,100,3)));
    for(int i=0;i<samples.size();i++) {
      var sample=samples.get(i);var direct=sample.getDirectEntity();
      if(direct!=null)direct.setPos(1,2,3);
      var before=sample.getSourcePosition();
      if(direct!=null)direct.setPos(-4,5,-6);
      int directIndex=-1,causeIndex=-1;
      for(int actor=0;actor<actors.length;actor++) {
        if(actors[actor]==direct)directIndex=actor;
        if(actors[actor]==sample.getEntity())causeIndex=actor;
      }
      System.out.println("damage-source\t"+names.get(i)+"\t"+sample.typeHolder().unwrapKey().orElseThrow().identifier()+"\t"+
        directIndex+"\t"+causeIndex+"\t"+sample.isDirect()+"\t"+sample.scalesWithDifficulty()+"\t"+sample.isCreativePlayer()+"\t"+
        (sample.sourcePositionRaw()==null?"none":TrackingVectors.vec(sample.sourcePositionRaw()))+"\t"+
        (before==null?"none":TrackingVectors.vec(before))+"\t"+
        (sample.getSourcePosition()==null?"none":TrackingVectors.vec(sample.getSourcePosition())));
    }
  }
}
