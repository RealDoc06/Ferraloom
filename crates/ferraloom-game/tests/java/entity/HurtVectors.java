import java.lang.reflect.*;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.advancements.triggers.CriteriaTriggers;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.sounds.*;
import net.minecraft.stats.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.*;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.effect.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.scores.Scoreboard;
import static org.mockito.Mockito.*;

/** Actual hurt and actuallyHurt. A deliberate throw after the virtual actuallyHurt
 * measures the ordered parent prefix without depending on tracker support. */
public final class HurtVectors {
  static final class Boundary extends RuntimeException {}
  static final class ServerFixture extends ServerLevel {
    List<String> events;LivingEntity victim;boolean stop;Level original;
    GameRules rules;Difficulty difficulty;ServerScoreboard board;DamageSources sources;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public net.minecraft.world.level.chunk.ChunkAccess getChunk(int x,int z,net.minecraft.world.level.chunk.status.ChunkStatus status,boolean load){return original.getChunk(x,z,status,load);}
    @Override public net.minecraft.world.level.border.WorldBorder getWorldBorder(){return original.getWorldBorder();}
    @Override public net.minecraft.world.level.BlockGetter getChunkForCollisions(int x,int z){return original.getChunkForCollisions(x,z);}
    @Override public net.minecraft.world.level.block.state.BlockState getBlockState(net.minecraft.core.BlockPos pos){return original.getBlockState(pos);}
    @Override public GameRules getGameRules(){return rules;}
    @Override public Difficulty getDifficulty(){return difficulty;}
    @Override public ServerScoreboard getScoreboard(){return board;}
    @Override public DamageSources damageSources(){return sources;}
    @Override public long getGameTime(){return 123;}
    @Override public void broadcastDamageEvent(Entity entity,DamageSource source){events.add("broadcast:"+snapshot(victim));}
    @Override public void gameEvent(Holder<GameEvent> type,Vec3 pos,GameEvent.Context context){events.add("event:"+BuiltInRegistries.GAME_EVENT.getKey(type.value())+":"+snapshot(victim));}
    @Override public void playSeededSound(Entity except,double x,double y,double z,Holder<SoundEvent> sound,SoundSource source,float volume,float pitch,long seed){events.add("sound:"+sound.value().location()+":"+source.ordinal()+":"+f(volume)+":"+f(pitch)+":"+Long.toHexString(seed));}
  }
  static final class Receiver extends ServerPlayer {
    ServerFixture world;PlayerAdvancements tracker;GameType mode;
    Receiver(){super(null,null,null,null);}
    @Override public ServerLevel level(){return world;}
    @Override public GameType gameMode(){return mode;}
    @Override public PlayerAdvancements getAdvancements(){world.events.add("criterion:"+snapshot(this));return tracker;}
    @Override public void awardStat(Stat<?> stat,int amount){super.awardStat(stat,amount);world.events.add("stat:"+stat.getName()+":"+amount+":"+snapshot(this));}
    void shoulders(){setShoulderEntityLeft(new net.minecraft.nbt.CompoundTag());setShoulderEntityRight(new net.minecraft.nbt.CompoundTag());}
    void rawAbsorption(float value){internalSetAbsorptionAmount(value);}
    @Override protected void actuallyHurt(ServerLevel level,DamageSource source,float damage){super.actuallyHurt(level,source,damage);if(world.stop)throw new Boundary();}
    void actual(DamageSource source,float damage){super.actuallyHurt(world,source,damage);}
    void hurtSound(DamageSource source){super.playHurtSound(source);}
  }
  static final class CowReceiver extends Cow {
    CowReceiver(Level level){super(EntityTypes.COW,level);}
    void rawAbsorption(float value){internalSetAbsorptionAmount(value);}
    @Override protected void actuallyHurt(ServerLevel level,DamageSource source,float damage){super.actuallyHurt(level,source,damage);if(((ServerFixture)level).stop)throw new Boundary();}
    void actual(ServerLevel level,DamageSource source,float damage){super.actuallyHurt(level,source,damage);}
    void hurtSound(DamageSource source){super.playHurtSound(source);}
  }
  static String f(float value){return ExperienceVectors.f(value);}
  static String stats(LivingEntity living){
    if(!(living instanceof Receiver player))return "-";
    return player.getStats().getValue(Stats.CUSTOM.get(Stats.DAMAGE_TAKEN))+","+player.getStats().getValue(Stats.CUSTOM.get(Stats.DAMAGE_ABSORBED))+","+player.getStats().getValue(Stats.CUSTOM.get(Stats.DAMAGE_RESISTED));
  }
  static String snapshot(LivingEntity living){
    try{return f(living.getHealth())+","+f(living.getAbsorptionAmount())+","+f(ExperienceVectors.field(LivingEntity.class,"lastHurt").getFloat(living))+","+living.damageCooldownTime+","+living.hurtTime+","+stats(living);}
    catch(Exception error){throw new RuntimeException(error);}
  }
  static String result(LivingEntity living){
    try{
      var source=(DamageSource)ExperienceVectors.field(LivingEntity.class,"lastDamageSource").get(living);
      var mob=(EntityReference<?>)ExperienceVectors.field(LivingEntity.class,"lastHurtByMob").get(living);
      var player=(EntityReference<?>)ExperienceVectors.field(LivingEntity.class,"lastHurtByPlayer").get(living);
      String food=living instanceof Receiver p?f(ExperienceVectors.field(net.minecraft.world.food.FoodData.class,"exhaustionLevel").getFloat(p.getFoodData())):"-";
      return snapshot(living)+"\t"+DropVectors.vector(living.getDeltaMovement())+"\t"+living.needsSync+"\t"+living.syncVelocity+"\t"+food+"\t"+(source==null?"-":source.typeHolder().unwrapKey().orElseThrow().identifier())+"\t"+(mob!=null)+"\t"+(player!=null);
    }catch(Exception error){throw new RuntimeException(error);}
  }
  static ServerFixture server(Level original,sun.misc.Unsafe allocator)throws Exception {
    var level=(ServerFixture)allocator.allocateInstance(ServerFixture.class);DropVectors.copyFields(original,level,Level.class);
    level.original=original;level.rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);level.difficulty=Difficulty.NORMAL;level.board=new ServerScoreboard(mock(MinecraftServer.class));level.sources=original.damageSources();level.events=new ArrayList<>();level.stop=true;
    ExperienceVectors.field(Level.class,"soundSeedGenerator").set(level,RandomSource.create(777));
    return level;
  }
  static Receiver player(Level original,ServerFixture level,sun.misc.Unsafe allocator)throws Exception {
    var player=(Receiver)allocator.allocateInstance(Receiver.class);DropVectors.copyFields(new LivingVectors(original),player,Player.class);
    ExperienceVectors.field(LivingEntity.class,"combatTracker").set(player,new CombatTracker(player));
    ExperienceVectors.field(net.minecraft.network.syncher.SynchedEntityData.class,"entity").set(player.getEntityData(),player);
    ExperienceVectors.field(net.minecraft.world.entity.player.Inventory.class,"player").set(player.getInventory(),player);
    ExperienceVectors.field(Entity.class,"level").set(player,level);player.world=level;player.mode=GameType.SURVIVAL;player.tracker=mock(PlayerAdvancements.class);
    var minecraft=mock(MinecraftServer.class);var counter=new ServerStatsCounter(minecraft,Path.of("unused-entity-hurt-counter.json"));
    ExperienceVectors.field(ServerPlayer.class,"stats").set(player,counter);
    player.connection=mock(ServerGamePacketListenerImpl.class);when(player.connection.hasClientLoaded()).thenReturn(true);
    doAnswer(call->{
      Packet<?> packet=call.getArgument(0);
      String value=packet instanceof ClientboundPlayerCombatEnterPacket?"enter":packet instanceof ClientboundHurtAnimationPacket p?"animation:"+f(p.yaw()):"other:"+packet.getClass().getSimpleName();
      level.events.add("packet:"+value+":"+snapshot(player));return null;
    }).when(player.connection).send(any(Packet.class));
    player.shoulders();
    return player;
  }
  static DamageSource source(String name,ServerFixture level,LivingEntity attacker){
    return switch(name){case "fall"->level.damageSources().fall();case "lava"->level.damageSources().lava();case "freeze"->level.damageSources().freeze();case "in_wall"->level.damageSources().inWall();case "drown"->level.damageSources().drown();case "mob_attack"->level.damageSources().mobAttack(attacker);case "generic_kill"->level.damageSources().genericKill();default->level.damageSources().generic();};
  }
  static void memoryTicks(Level original,sun.misc.Unsafe allocator)throws Exception {
    int[][] cases={{0,0,0,0,100},{1,1,1,0,100},{10,20,2,0,101},{-1,-1,-1,0,101},{10,20,1,0,Integer.MIN_VALUE},{10,20,2,Integer.MAX_VALUE,Integer.MIN_VALUE}};
    for(String kind:List.of("cow","player"))for(boolean alive:List.of(false,true))for(int index=0;index<cases.length;index++) {
      var values=cases[index];LivingEntity victim=kind.equals("player")?player(original,server(original,allocator),allocator):new CowReceiver(original);victim.setPos(8.5,64,8.5);
      var attacker=new CowReceiver(original);attacker.setHealth(alive?10:0);
      var player=new LivingVectors(original);victim.tickCount=values[3];victim.setLastHurtByMob(attacker);victim.setLastHurtMob(attacker);victim.setLastHurtByPlayer(player,values[2]);
      victim.hurtTime=values[0];victim.damageCooldownTime=values[1];victim.tickCount=values[4];victim.baseTick();
      System.out.println("hurt-memory-tick\t"+kind+"\t"+alive+"\t"+index+"\t"+Arrays.toString(values).replace(" ","")+"\t"+victim.hurtTime+","+victim.damageCooldownTime+","+victim.getLastHurtByPlayerMemoryTime()+","+(ExperienceVectors.field(LivingEntity.class,"lastHurtByPlayer").get(victim)!=null)+","+(ExperienceVectors.field(LivingEntity.class,"lastHurtByMob").get(victim)!=null)+","+victim.getLastHurtByMobTimestamp()+","+(victim.getLastHurtMob()!=null)+","+victim.getLastHurtMobTimestamp());
    }
  }
  static void responses(Level original,sun.misc.Unsafe allocator)throws Exception {
    for(double power:new double[]{0,0.4000000059604645,1})for(double resistance:new double[]{0,0.5,1})for(boolean ground:new boolean[]{false,true})for(boolean zero:new boolean[]{false,true}) {
      var level=server(original,allocator);var victim=new CowReceiver(original);level.victim=victim;
      ExperienceVectors.field(Entity.class,"level").set(victim,level);
      victim.getAttribute(Attributes.KNOCKBACK_RESISTANCE).setBaseValue(resistance);victim.setOnGround(ground);victim.setDeltaMovement(0.25,-0.125,0.5);victim.getRandom().setSeed(12345);
      double x=zero?0:2,z=zero?0:-3;victim.knockback(power,x,z,level.damageSources().generic(),4);
      System.out.println("hurt-knockback\t"+Double.toString(power)+"\t"+Double.toString(resistance)+"\t"+ground+"\t"+zero+"\t"+DropVectors.vector(victim.getDeltaMovement())+"\t"+victim.needsSync+"\t"+Long.toHexString(victim.getRandom().nextLong()));
    }
    for(String kind:List.of("player","cow","baby_cow"))for(String name:List.of("generic","fall","lava","drown","freeze"))for(boolean silent:new boolean[]{false,true}) {
      var level=server(original,allocator);LivingEntity victim;
      if(kind.equals("player"))victim=player(original,level,allocator);
      else {victim=new CowReceiver(original);ExperienceVectors.field(Entity.class,"level").set(victim,level);if(kind.equals("baby_cow"))ExperienceVectors.field(AgeableMob.class,"age").setInt(victim,-24000);}
      level.victim=victim;victim.setPos(8.5,64,8.5);victim.setSilent(silent);victim.getRandom().setSeed(12345);level.events.clear();
      var source=source(name,level,victim);
      if(victim instanceof Receiver player)player.hurtSound(source);else((CowReceiver)victim).hurtSound(source);
      System.out.println("hurt-sound\t"+kind+"\t"+name+"\t"+silent+"\t"+(level.events.isEmpty()?"-":String.join(";",level.events))+"\t"+Long.toHexString(victim.getRandom().nextLong()));
    }
  }
  public static void run(Level original)throws Exception {
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);var allocator=(sun.misc.Unsafe)field.get(null);
    memoryTicks(original,allocator);
    responses(original,allocator);
    // damage, armor, toughness, raw absorption, max absorption, resistance,
    // cooldown, lastHurt. Nonfinite full hits stop before the lethal lifecycle.
    float[][] cases={{4,0,0,0,0,-1,0,0},{4,20,0,0,0,-1,0,0},{20,20,8,0,0,0,0,0},{6,0,0,5,20,-1,0,0},{6,10,4,5,20,1,0,0},{1,0,0,6.25f,0,-1,0,0},{-0.0f,0,0,0,0,-1,0,0},{-1,0,0,0,0,-1,0,0},{Float.NaN,0,0,0,0,-1,0,0},{Float.POSITIVE_INFINITY,20,8,0,0,-1,0,0},{5.5f,7.25f,1.5f,1.25f,4,0,11,3},{3,0,0,0,0,-1,11,5},{7,0,0,0,0,-1,11,3},{7,0,0,0,0,-1,10,3},{4,0,0,0,0,4,0,0},{4,0,0,0,0,-2,0,0}};
    for(String kind:List.of("player","cow"))for(String operation:List.of("actual","hurt","increment"))for(int index=0;index<cases.length;index++){
      if(operation.equals("increment")&&(index<10||index>12))continue;
      var setup=cases[index];var level=server(original,allocator);LivingEntity victim;
      if(kind.equals("player"))victim=player(original,level,allocator);else {victim=new CowReceiver(original);ExperienceVectors.field(Entity.class,"level").set(victim,level);}
      level.victim=victim;level.stop=!operation.equals("increment");victim.setId(1);victim.setPos(8.5,64,8.5);victim.setYRot(37.25f);victim.setYHeadRot(37.25f);victim.setOnGround(index%2==0);victim.setDeltaMovement(0.25,-0.125,0.5);victim.tickCount=17;
      victim.getAttribute(Attributes.ARMOR).setBaseValue(setup[1]);victim.getAttribute(Attributes.ARMOR_TOUGHNESS).setBaseValue(setup[2]);victim.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(setup[4]);
      if(victim instanceof Receiver player)player.rawAbsorption(setup[3]);else ((CowReceiver)victim).rawAbsorption(setup[3]);
      if(setup[5]>=0||setup[5]==-2){var effect=setup[5]==-2?MobEffects.FIRE_RESISTANCE:MobEffects.RESISTANCE;((Map<Holder<MobEffect>,MobEffectInstance>)ExperienceVectors.field(LivingEntity.class,"activeEffects").get(victim)).put(effect,new MobEffectInstance(effect,100,setup[5]<0?0:(int)setup[5]));}
      victim.damageCooldownTime=(int)setup[6];ExperienceVectors.field(LivingEntity.class,"lastHurt").setFloat(victim,setup[7]);
      var attacker=new Cow(EntityTypes.COW,original);attacker.setId(2);attacker.setPos(10.5,64,8.5);
      String name=index==15?"lava":index==14?"freeze":index%4==0?"mob_attack":index%4==1?"fall":"generic";
      var source=source(name,level,attacker);victim.getRandom().setSeed(12345);level.events.clear();
      String accepted;
      try {if(operation.equals("actual")){if(victim instanceof Receiver player)player.actual(source,setup[0]);else((CowReceiver)victim).actual(level,source,setup[0]);accepted="actual";}else accepted=Boolean.toString(victim.hurtServer(level,source,setup[0]));}
      catch(Boundary boundary){accepted="boundary";}
      var inputs=new StringJoiner(",");for(float value:setup)inputs.add(f(value));
      System.out.println("hurt-pipeline\t"+kind+"\t"+operation+"\t"+index+"\t"+name+"\t"+inputs+"\t"+(index%2==0)+"\t"+accepted+"\t"+result(victim)+"\t"+(level.events.isEmpty()?"-":String.join(";",level.events))+"\t"+Long.toHexString(victim.getRandom().nextLong()));
    }
  }
}
