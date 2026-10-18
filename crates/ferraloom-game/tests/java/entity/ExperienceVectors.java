import java.lang.reflect.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.server.level.*;
import net.minecraft.core.Holder;
import net.minecraft.sounds.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.storage.TagValueInput;

/** Actual Player arithmetic and ServerPlayer cache invalidation, without IO. */
public final class ExperienceVectors {
  static final class ServerFixture extends ServerLevel {
    StringJoiner sounds;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public void playSeededSound(Entity except,double x,double y,double z,Holder<SoundEvent> sound,SoundSource source,float volume,float pitch,long seed) {
      sounds.add(sound.value().location()+":"+source.ordinal()+":"+f(volume)+":"+f(pitch)+":"+Long.toHexString(seed));
    }
  }
  static Field field(Class<?> owner,String name)throws Exception {var field=owner.getDeclaredField(name);field.setAccessible(true);return field;}
  static String f(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
  static String result(Player player)throws Exception {
    return player.experienceLevel+"\t"+f(player.experienceProgress)+"\t"+player.totalExperience+"\t"+player.getScore()+"\t"+field(Player.class,"lastLevelUpTime").getInt(player)+"\t"+field(ServerPlayer.class,"lastSentExp").getInt(player);
  }
  public static void run(Level level)throws Exception {
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    int[] curve={Integer.MIN_VALUE,-4,-3,-1,0,1,14,15,16,29,30,31,100,238609312,Integer.MAX_VALUE};
    var base=new LivingVectors(level);
    for(int value:curve){base.experienceLevel=value;System.out.println("xp-curve\t"+value+"\t"+base.getXpNeededForNextLevel());}
    String[][] cases={
      {"points","0","0","0","0","0","0","0"},
      {"points","0","0","0","0","1","0","0"},
      {"points","0","0","0","0","7","0","0"},
      {"points","0","0","0","0","8","0","0"},
      {"points","14","0.75","200","5","37","0","0"},
      {"points","29","0.99","10000","100","100","0","0"},
      {"points","32","0.2","12000","300","-200","0","0"},
      {"points","1","0.01","1","0","-500","0","0"},
      {"points","0","0","2147483647","2147483647","1","0","0"},
      {"points","0","0","1","-2147483648","-2","0","0"},
      {"points","0","-0.0","0","0","0","0","0"},
      {"points","0","NaN","0","0","1","0","0"},
      {"points","0","1.5","0","0","0","0","0"},
      {"points","0","0","0","0","10000","201","0"},
      {"levels","4","0.25","100","20","1","100","0"},
      {"levels","4","0.25","100","20","1","101","0"},
      {"levels","4","0.25","100","20","1","301","201"},
      {"levels","4","0.25","100","20","1","302","201"},
      {"levels","29","0.25","100","20","1","201","0"},
      {"levels","34","0.25","100","20","1","201","0"},
      {"levels","2147483645","0.5","100","20","10","201","0"},
      {"levels","10","0.5","100","20","-20","201","0"},
      {"levels","-5","0.5","100","20","0","201","0"},
      {"levels","2147483647","0.5","100","20","-2147483648","201","0"},
      {"levels","4","0.25","100","20","1","16777316","16777216"}
    };
    for(String[] setup:cases) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);server.sounds=new StringJoiner(",");
      field(Level.class,"soundSeedGenerator").set(server,net.minecraft.util.RandomSource.create(0));
      var player=(ServerPlayer)allocator.allocateInstance(ServerPlayer.class);
      var source=new LivingVectors(level);
      // Populate the real embedded parent fields from an initialized Player;
      // only the requested XP methods execute on the ServerPlayer receiver.
      for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(Field field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())) {
        field.setAccessible(true);field.set(player,field.get(source));
      }
      field(Entity.class,"level").set(player,server);
      player.experienceLevel=Integer.parseInt(setup[1]);player.experienceProgress=Float.parseFloat(setup[2]);
      player.totalExperience=Integer.parseInt(setup[3]);player.setScore(Integer.parseInt(setup[4]));
      player.tickCount=Integer.parseInt(setup[6]);field(Player.class,"lastLevelUpTime").setInt(player,Integer.parseInt(setup[7]));
      field(ServerPlayer.class,"lastSentExp").setInt(player,23);
      if(setup[0].equals("points"))player.giveExperiencePoints(Integer.parseInt(setup[5]));else player.giveExperienceLevels(Integer.parseInt(setup[5]));
      System.out.println("xp-award\t"+String.join("\t",setup)+"\t"+result(player)+"\t"+(server.sounds.length()==0?"-":server.sounds));
    }
    for(String scenario:List.of("absent","normal","negative","wrong","nan")) {
      var tag=new CompoundTag();
      if(scenario.equals("normal")){tag.putFloat("XpP",0.75f);tag.putInt("XpLevel",17);tag.putInt("XpTotal",777);tag.putInt("Score",123);}
      if(scenario.equals("negative")){tag.putFloat("XpP",-0.0f);tag.putInt("XpLevel",-5);tag.putInt("XpTotal",-9);tag.putInt("Score",-20);}
      if(scenario.equals("wrong")){for(String key:List.of("XpP","XpLevel","XpTotal","Score"))tag.putString(key,"bad");}
      if(scenario.equals("nan"))tag.putFloat("XpP",Float.NaN);
      var player=new LivingVectors(level);player.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      System.out.println("xp-nbt\t"+scenario+"\t"+player.experienceLevel+"\t"+f(player.experienceProgress)+"\t"+player.totalExperience+"\t"+player.getScore());
    }
  }
}
