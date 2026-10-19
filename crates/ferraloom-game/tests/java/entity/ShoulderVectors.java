import java.lang.reflect.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;

/** Actual ServerPlayer setters and ambient RNG gates, preserving full server
 * tags while the client metadata contains only an optional parrot variant. */
public final class ShoulderVectors {
  static CompoundTag tag(String id,Integer variant,Boolean silent) {
    var tag=new CompoundTag();if(id!=null)tag.putString("id",id);if(variant!=null)tag.putInt("Variant",variant);if(silent!=null)tag.putBoolean("Silent",silent);return tag;
  }
  static ServerPlayer receiver(Level level,sun.misc.Unsafe allocator)throws Exception {
    var player=(ServerPlayer)allocator.allocateInstance(ServerPlayer.class);var source=new TravelVectors(level);
    for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(var field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){field.setAccessible(true);field.set(player,field.get(source));}
    return player;
  }
  public static void run(Level level)throws Exception {
    var f=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");f.setAccessible(true);var allocator=(sun.misc.Unsafe)f.get(null);
    var left=ServerPlayer.class.getDeclaredMethod("setShoulderEntityLeft",CompoundTag.class);left.setAccessible(true);
    var right=ServerPlayer.class.getDeclaredMethod("setShoulderEntityRight",CompoundTag.class);right.setAccessible(true);
    var player=receiver(level,allocator);right.invoke(player,new CompoundTag());player.getEntityData().packDirty();
    var tags=List.of(tag(null,null,null),tag("minecraft:parrot",0,null),tag("minecraft:parrot",0,true),tag("minecraft:parrot",4,null),tag("parrot",99,null),tag("minecraft:parrot",-7,null),tag("minecraft:pig",2,null),tag("minecraft:parrot",null,null),tag(null,null,null));
    int index=0;
    for(var tag:tags) {
      left.invoke(player,tag);
      var variant=player.getShoulderParrotLeft();
      System.out.println("shoulder-metadata\t"+(index++)+"\t"+variant.map(v->""+v.getId()).orElse("-")+"\t"+(player.getEntityData().packDirty()!=null)+"\t"+player.getShoulderEntityLeft().equals(tag));
    }
    int zero=0;while(net.minecraft.util.RandomSource.create(zero).nextInt(200)!=0)zero++;
    for(int scenario=0;scenario<8;scenario++) {
      player=receiver(level,allocator);
      var a=tag(null,null,null);var b=tag(null,null,null);long seed=0;
      if(scenario==1)a=tag("minecraft:parrot",2,true);
      if(scenario==2)a=tag("minecraft:parrot",2,false);
      if(scenario==3){a=tag("minecraft:parrot",2,false);b=tag("minecraft:parrot",4,false);}
      if(scenario==4){a=tag("minecraft:pig",null,false);seed=zero;}
      if(scenario==5){a=tag(null,null,false);seed=zero;}
      if(scenario==6){a=tag("minecraft:parrot",1,true);b=tag("minecraft:parrot",3,false);}
      if(scenario==7){a=tag("minecraft:parrot",1,false);a.putString("Silent","invalid");}
      left.invoke(player,a);right.invoke(player,b);player.getRandom().setSeed(seed);player.getEntityData().packDirty();
      player.handleShoulderEntities();
      System.out.println("shoulder-rng\t"+scenario+"\t"+seed+"\t"+player.getRandom().nextLong()+"\t"+(player.getEntityData().packDirty()!=null));
    }
  }
}
