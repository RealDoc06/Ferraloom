import java.lang.reflect.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.nbt.*;
import net.minecraft.server.level.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.*;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.food.FoodData;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.*;

/** Native FoodData methods and real ServerPlayer peaceful regeneration. */
public final class FoodVectors {
  static net.minecraft.core.HolderLookup.Provider lookup;
  static final class ServerFixture extends ServerLevel {
    Difficulty difficulty;GameRules rules;DamageSources sources;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public Difficulty getDifficulty(){return difficulty;}
    @Override public GameRules getGameRules(){return rules;}
    @Override public DamageSources damageSources(){return sources;}
  }
  static final class Receiver extends ServerPlayer {
    ServerFixture server;StringJoiner events;
    Receiver(){super(null,null,new GameProfile(new UUID(3,4),"food"),null);}
    @Override public ServerLevel level(){return server;}
    @Override public void heal(float amount){events.add("heal:"+f(amount)+":"+state(getFoodData()));super.heal(amount);}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount){events.add("hurt:"+source.type().msgId()+":"+f(amount)+":"+state(getFoodData()));return false;}
    void regenerate(){tickRegeneration();}
  }
  static String f(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
  static String state(FoodData food){var output=TagValueOutput.createWithoutContext(ProblemReporter.DISCARDING);food.addAdditionalSaveData(output);var tag=output.buildResult();return tag.getIntOr("foodLevel",0)+","+f(tag.getFloatOr("foodSaturationLevel",0))+","+f(tag.getFloatOr("foodExhaustionLevel",0))+","+tag.getIntOr("foodTickTimer",0);}
  static CompoundTag tag(int food,float saturation,float exhaustion,int timer){var tag=new CompoundTag();tag.putInt("foodLevel",food);tag.putFloat("foodSaturationLevel",saturation);tag.putFloat("foodExhaustionLevel",exhaustion);tag.putInt("foodTickTimer",timer);return tag;}
  static void load(FoodData food,CompoundTag tag){food.readAdditionalSaveData(TagValueInput.create(ProblemReporter.DISCARDING,lookup,tag));}
  public static void run(Level level)throws Exception {
    lookup=level.registryAccess();
    for(float saturation:new float[]{0f,-0f,2.5f,Float.NaN,Float.POSITIVE_INFINITY}) {
      var food=new FoodData();food.setSaturation(saturation);
      ExperienceVectors.field(FoodData.class,"exhaustionLevel").setFloat(food,saturation);
      System.out.println("food-save\t"+f(saturation)+"\t"+f(food.getSaturationLevel())+"\t"+state(food));
    }
    for(String name:List.of("default","loaded","negative","wrong","numeric","nan")) {
      var tag=tag(7,1.25f,3.5f,17);
      switch(name){case "default"->tag=new CompoundTag();case "negative"->tag=tag(-8,-0f,-7.5f,-9);case "wrong"->{tag.putString("foodLevel","x");tag.putString("foodTickTimer","x");tag.putString("foodSaturationLevel","x");tag.putString("foodExhaustionLevel","x");}case "numeric"->{tag.putDouble("foodLevel",17.8);tag.putDouble("foodSaturationLevel",2.7);tag.putLong("foodTickTimer",Integer.MAX_VALUE+1L);}case "nan"->tag=tag(99,Float.NaN,Float.POSITIVE_INFINITY,Integer.MAX_VALUE);}
      var food=new FoodData();load(food,tag);
      System.out.println("food-load\t"+name+"\t"+state(food)+"\t"+food.needsFood()+"\t"+food.hasEnoughFood());
    }
    int[] nutrition={-7,0,1,9,20,Integer.MAX_VALUE};float[] modifiers={-0.0f,0.3f,1.2f,Float.NaN};
    for(int n:nutrition)for(float modifier:modifiers){var food=new FoodData();food.eat(n,modifier);food.addExhaustion(modifier*100f);System.out.println("food-eat\t"+n+"\t"+f(modifier)+"\t"+state(food));}
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    float[][] setups={{20,5,0,9,19},{20,7,0,9,19},{20,5,4,9,19},{20,5,Math.nextUp(4f),9,19},{20,0,5,79,19},{18,0,0,79,19},{17,0,0,79,19},{0,0,0,79,20},{0,0,0,79,10},{0,0,0,79,1},{0,0,0,79,0},{20,5,0,Integer.MAX_VALUE,19},{20,Float.NaN,Float.NaN,9,19},{20,5,0,9,20}};
    for(int i=0;i<setups.length;i++)for(var difficulty:Difficulty.values())for(boolean natural:List.of(false,true)) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);server.difficulty=difficulty;server.rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);server.rules.set(GameRules.NATURAL_HEALTH_REGENERATION,natural,null);server.sources=level.damageSources();
      var player=(Receiver)allocator.allocateInstance(Receiver.class);var source=new TravelVectors(level);
      for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(var field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){field.setAccessible(true);field.set(player,field.get(source));}
      player.server=server;player.events=new StringJoiner(";");var setup=setups[i];var food=new FoodData();load(food,tag((int)setup[0],setup[1],setup[2],(int)setup[3]));
      ExperienceVectors.field(Player.class,"foodData").set(player,food);player.setHealth(setup[4]);String before=state(food);
      food.tick(player);
      System.out.println("food-tick\t"+i+"\t"+difficulty.getId()+"\t"+natural+"\t"+before+"\t"+f(setup[4])+"\t"+state(food)+"\t"+f(player.getHealth())+"\t"+(player.events.length()==0?"-":player.events));
    }
    for(var difficulty:Difficulty.values())for(boolean natural:List.of(false,true))for(int ticks:new int[]{9,10,19,20,40}) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);server.difficulty=difficulty;server.rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);server.rules.set(GameRules.NATURAL_HEALTH_REGENERATION,natural,null);
      var player=(Receiver)allocator.allocateInstance(Receiver.class);var source=new TravelVectors(level);
      for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(var field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){field.setAccessible(true);field.set(player,field.get(source));}
      player.server=server;player.events=new StringJoiner(";");load(player.getFoodData(),tag(19,19.5f,3.5f,7));player.setHealth(18.75f);player.tickCount=ticks;player.regenerate();
      System.out.println("food-peaceful\t"+difficulty.getId()+"\t"+natural+"\t"+ticks+"\t"+state(player.getFoodData())+"\t"+f(player.getHealth())+"\t"+(player.events.length()==0?"-":player.events));
    }
  }
}
