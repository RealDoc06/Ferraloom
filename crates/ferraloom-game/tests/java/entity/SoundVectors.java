import java.lang.reflect.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.sounds.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;

/** Real sound call sites, including silent/player overrides and particle RNG. */
public final class SoundVectors extends Player {
  boolean spectator;
  SoundVectors(Level level) {super(level,new GameProfile(new UUID(1,2),"sound"));}
  @Override public GameType gameMode() {return spectator?GameType.SPECTATOR:GameType.SURVIVAL;}
  void wet() {wasTouchingWater=true;}
  static String f(float value) {return Integer.toHexString(Float.floatToRawIntBits(value));}
  static String vec(double x,double y,double z) {return Long.toHexString(Double.doubleToRawLongBits(x))+","+Long.toHexString(Double.doubleToRawLongBits(y))+","+Long.toHexString(Double.doubleToRawLongBits(z));}
  static String capture(Entity except,double x,double y,double z,Holder<SoundEvent> sound,SoundSource source,float volume,float pitch,long seed) {
    return (except==null?"none":except.getId())+":"+sound.value().location()+":"+source.ordinal()+":"+((int)(x*8))+","+((int)(y*8))+","+((int)(z*8))+":"+f(volume)+":"+f(pitch)+":"+Long.toHexString(seed);
  }
  static String capture_game_event(Holder<net.minecraft.world.level.gameevent.GameEvent> event,Vec3 pos,net.minecraft.world.level.gameevent.GameEvent.Context context) {
    return BuiltInRegistries.GAME_EVENT.getKey(event.value())+":"+(context.sourceEntity()==null?"none":context.sourceEntity().getId())+":"+(context.affectedState()==null?"none":Block.getId(context.affectedState()))+":"+vec(pos.x,pos.y,pos.z);
  }
  static Method method(String name,Class<?>... args) throws Exception {var method=Entity.class.getDeclaredMethod(name,args);method.setAccessible(true);return method;}
  static void put(Level level,BlockPos pos,Block block) {level.getChunkAt(pos).getSection(level.getSectionIndex(pos.getY())).setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,block.defaultBlockState());}
  @SuppressWarnings("unchecked") public static void run(Level level) throws Exception {
    var captureField=level.getClass().getDeclaredField("captureEntitySounds");captureField.setAccessible(true);captureField.setBoolean(level,true);
    var soundField=level.getClass().getDeclaredField("soundCalls");soundField.setAccessible(true);var sounds=(List<String>)soundField.get(level);
    var eventField=level.getClass().getDeclaredField("gameEvents");eventField.setAccessible(true);var events=(List<String>)eventField.get(level);
    var seedField=Level.class.getDeclaredField("soundSeedGenerator");seedField.setAccessible(true);var seeds=(RandomSource)seedField.get(level);
    var step=method("walkingStepSound",BlockPos.class,BlockState.class);var swim=method("waterSwimSound");var splash=method("doWaterSplashEffect");var sprint=method("spawnSprintParticle");var extinguish=method("playEntityOnFireExtinguishedSound");
    var pos=new BlockPos(8,63,8);
    for(String name:List.of("base-step","base-silent-step","player-silent-step","player-snow","player-lichen","player-water-step","player-move-step","crystal","base-swim","player-swim","base-splash","player-splash-high","player-splash-spectator","orb-splash","orb-swim","item-swim","base-splash-silent","controller-splash","sprint-stone","sprint-barrier","sprint-water","sprint-crouch","sprint-player-spectator","extinguish-player-silent")) {
      put(level,pos,Blocks.STONE);put(level,pos.above(),Blocks.AIR);sounds.clear();events.clear();seeds.setSeed(321);
      Entity entity=name.startsWith("player-")||name.equals("sprint-player-spectator")||name.startsWith("extinguish-player")?new SoundVectors(level):name.startsWith("orb-")?new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level):name.equals("item-swim")?new net.minecraft.world.entity.item.ItemEntity(EntityTypes.ITEM,level):name.equals("controller-splash")?new Cow(EntityTypes.COW,level):new PrimedTnt(EntityTypes.TNT,level);
      entity.setId(1);entity.setPos(8.5,64,8.5);entity.setDeltaMovement(0.25,-0.5,0.125);entity.getRandom().setSeed(123);
      if(name.contains("silent"))entity.setSilent(true);
      if(name.equals("player-snow"))put(level,pos.above(),Blocks.SNOW);
      if(name.equals("player-lichen"))put(level,pos.above(),Blocks.GLOW_LICHEN);
      if(name.equals("player-water-step"))((SoundVectors)entity).wet();
      if(name.equals("player-splash-high"))entity.setDeltaMovement(2,-3,1);
      if(name.endsWith("spectator"))((SoundVectors)entity).spectator=true;
      if(name.equals("controller-splash")) {var rider=new Cow(EntityTypes.COW,level);rider.setDeltaMovement(2,-3,1);rider.startRiding(entity,true,false);sounds.clear();events.clear();}
      if(name.equals("crystal")) {put(level,pos,Blocks.AMETHYST_BLOCK);for(int tick:new int[]{0,20,21,40,100}) {entity.tickCount=tick;step.invoke(entity,pos,Blocks.AMETHYST_BLOCK.defaultBlockState());}}
      else if(name.equals("player-move-step")) {entity.moveDist=2;entity.setOnGround(true);entity.move(MoverType.SELF,new Vec3(0.4,-0.01,0));}
      else if(name.contains("step")||name.equals("player-snow")||name.equals("player-lichen"))step.invoke(entity,pos,Blocks.STONE.defaultBlockState());
      else if(name.endsWith("swim"))swim.invoke(entity);
      else if(name.contains("splash"))splash.invoke(entity);
      else if(name.startsWith("sprint-")) {
        if(name.equals("sprint-barrier"))put(level,pos,Blocks.BARRIER);
        if(name.equals("sprint-water"))put(level,pos,Blocks.WATER);
        if(name.equals("sprint-crouch"))entity.setPose(Pose.CROUCHING);
        entity.setSprinting(true);if(entity.canSpawnSprintParticle())sprint.invoke(entity);
      }
      else if(name.startsWith("extinguish"))extinguish.invoke(entity);
      System.out.println("entity-sound\t"+name+"\t"+String.join(";",sounds)+"\t"+Integer.toHexString(entity.getRandom().nextInt())+"\t"+(events.isEmpty()?"-":String.join(";",events)));
    }
  }
}
