import java.lang.reflect.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.server.PlayerAdvancements;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real virtual stuck/reset chain; the advancement getter observes the native
 * trigger before its empty listener map returns. No reset body is replaced. */
public final class StuckVectors {
  static final Vec3 OLD=new Vec3(-0.25,0.5,-0.0), SPEED=new Vec3(0.9,1.5,0.9);
  static final Vec3 START=new Vec3(2,80,-3), IMPACT=new Vec3(-0.0,70.25,3);
  static final class ResetPlayer extends ServerPlayer {
    PlayerAdvancements tracker;
    String beforeTrigger;
    int triggers;
    ResetPlayer(){super(null,null,null,null);}
    @Override public PlayerAdvancements getAdvancements(){
      try {
        if(!state(this).equals(beforeTrigger))throw new AssertionError("fall criterion ran after reset: "+state(this));
      }catch(Exception error){throw new RuntimeException(error);}
      triggers++;
      return tracker;
    }
  }
  static String vec(Vec3 value){return value==null?"-":DropVectors.vector(value);}
  static String state(Entity entity)throws Exception {
    String base=TravelVectors.d(entity.fallDistance)+"\t"+vec((Vec3)ExperienceVectors.field(Entity.class,"stuckSpeedMultiplier").get(entity));
    if(!(entity instanceof ResetPlayer player))return base+"\t-\t-\t-";
    return base+"\t"+vec((Vec3)ExperienceVectors.field(ServerPlayer.class,"startingToFallPosition").get(player))+"\t"+ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").getInt(player)+"\t"+vec((Vec3)ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").get(player));
  }
  public static void run(Level level)throws Exception {
    var allocatorField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");allocatorField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)allocatorField.get(null);
    double[] distances={0,5.25,Double.NaN};
    for(String kind:List.of("base","player"))for(String action:List.of("stuck","reset"))for(boolean flying:new boolean[]{false,true})for(int grace:new int[]{0,1,-1})for(int scenario=0;scenario<distances.length;scenario++){
      if(kind.equals("base")&&(flying||grace!=0))continue;
      Entity entity;
      boolean start=scenario!=0;
      float health=scenario==2?0:20;
      if(kind.equals("base"))entity=new PrimedTnt(EntityTypes.TNT,level);
      else {
        var player=(ResetPlayer)allocator.allocateInstance(ResetPlayer.class);
        DropVectors.copyFields(new LivingVectors(level),player,Player.class);
        player.tracker=mock(PlayerAdvancements.class);
        player.setHealth(health);player.getAbilities().flying=flying;
        ExperienceVectors.field(ServerPlayer.class,"startingToFallPosition").set(player,start?START:null);
        ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").setInt(player,grace);
        ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").set(player,IMPACT);
        entity=player;
      }
      entity.fallDistance=distances[scenario];entity.setPos(2,70,-3);entity.getRandom().setSeed(123);
      ExperienceVectors.field(Entity.class,"stuckSpeedMultiplier").set(entity,OLD);
      if(entity instanceof ResetPlayer player)player.beforeTrigger=state(player);
      if(action.equals("stuck"))entity.makeStuckInBlock(Blocks.POWDER_SNOW.defaultBlockState(),SPEED);else entity.resetFallDistance();
      if(entity instanceof ResetPlayer player){
        int expected=health>0&&start&&!(action.equals("stuck")&&flying)?1:0;
        if(player.triggers!=expected)throw new AssertionError("fall trigger count "+player.triggers+" != "+expected);
      }
      System.out.println("stuck-reset\t"+kind+"\t"+action+"\t"+flying+"\t"+grace+"\t"+TravelVectors.d(distances[scenario])+"\t"+ExperienceVectors.f(health)+"\t"+start+"\t"+state(entity)+"\t"+Long.toHexString(entity.getRandom().nextLong()));
      if(entity instanceof ResetPlayer player&&action.equals("reset")){
        int calls=player.triggers;player.resetFallDistance();
        if(player.triggers!=calls)throw new AssertionError("reset fired twice for one fall");
      }
    }
    for(double distance:new double[]{0,-1,5.25,Double.NaN}){
      var player=(ResetPlayer)allocator.allocateInstance(ResetPlayer.class);DropVectors.copyFields(new LivingVectors(level),player,Player.class);
      player.setPos(2,80,-3);player.fallDistance=distance;player.trackStartFallingPosition();
      player.setPos(2,70,-3);player.fallDistance=10;player.trackStartFallingPosition();
      System.out.println("fall-start\t"+TravelVectors.d(distance)+"\t"+vec((Vec3)ExperienceVectors.field(ServerPlayer.class,"startingToFallPosition").get(player)));
    }
    for(String scenario:List.of("absent","valid","negative","wrong","short","bytes","ints","longs","mixed")){
      var tag=new CompoundTag();
      if(!scenario.equals("absent"))tag.putInt("current_impulse_context_reset_grace_time",scenario.equals("negative")?-1:7);
      switch(scenario){
        case "valid","negative" -> tag.put("current_explosion_impact_pos",PersistenceVectors.doubles(-0.0,70.25,3));
        case "wrong" -> {tag.putString("current_explosion_impact_pos","bad");tag.putString("current_impulse_context_reset_grace_time","bad");}
        case "short" -> tag.put("current_explosion_impact_pos",PersistenceVectors.doubles(1,2));
        case "bytes" -> tag.putByteArray("current_explosion_impact_pos",new byte[]{-1,2,3});
        case "ints" -> tag.putIntArray("current_explosion_impact_pos",new int[]{-1,2,3});
        case "longs" -> tag.putLongArray("current_explosion_impact_pos",new long[]{Long.MAX_VALUE,2,3});
        case "mixed" -> {var list=new ListTag();list.add(IntTag.valueOf(1));list.add(DoubleTag.valueOf(2.5));list.add(LongTag.valueOf(3));tag.put("current_explosion_impact_pos",list);}
      }
      var living=new LivingVectors(level);living.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      int grace=ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").getInt(living);
      Vec3 impact=(Vec3)ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").get(living);
      living.tryResetCurrentImpulseContext();
      System.out.println("impulse-load\t"+scenario+"\t"+grace+"\t"+vec(impact)+"\t"+ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").getInt(living)+"\t"+vec((Vec3)ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").get(living)));
    }
    for(String action:List.of("enable","disable","max-grace","reset")){
      var living=new LivingVectors(level);living.setIgnoreFallDamageFromCurrentImpulse(true,IMPACT);
      switch(action){
        case "enable" -> living.setIgnoreFallDamageFromCurrentImpulse(true,START);
        case "disable" -> living.setIgnoreFallDamageFromCurrentImpulse(false,START);
        case "max-grace" -> {living.applyPostImpulseGraceTime(3);living.applyPostImpulseGraceTime(47);}
        case "reset" -> living.resetCurrentImpulseContext();
      }
      Vec3 impact=(Vec3)ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").get(living);
      String saved=impact==null?"-":vec(Vec3.CODEC.parse(NbtOps.INSTANCE,Vec3.CODEC.encodeStart(NbtOps.INSTANCE,impact).getOrThrow()).getOrThrow());
      System.out.println("impulse-state\t"+action+"\t"+ExperienceVectors.field(LivingEntity.class,"currentImpulseContextResetGraceTime").getInt(living)+"\t"+vec((Vec3)ExperienceVectors.field(LivingEntity.class,"currentImpulseImpactPos").get(living))+"\t"+living.isInPostImpulseGraceTime()+"\t"+living.isIgnoringFallDamageFromCurrentImpulse()+"\t"+saved);
    }
  }
}
