import java.lang.reflect.*;
import java.util.*;
import net.minecraft.server.level.*;
import net.minecraft.stats.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;

/** Actual ServerPlayer jump override: parent motion, stat, then ability-gated exhaustion. */
public final class ServerJumpVectors {
  static final class Receiver extends ServerPlayer {
    ServerLevel server;int awarded;String atAward;
    Receiver(){super(null,null,null,null);}
    @Override public ServerLevel level(){return server;}
    @Override protected float getBlockJumpFactor(){return 1;}
    @Override public void awardStat(Stat<?> stat,int amount){if(!stat.getValue().equals(Stats.JUMP))throw new AssertionError(stat);awarded+=amount;atAward=TrackingVectors.vec(getDeltaMovement())+":"+FoodVectors.state(getFoodData());}
  }
  public static void run(Level level)throws Exception {
    var f=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");f.setAccessible(true);var allocator=(sun.misc.Unsafe)f.get(null);
    var server=(FoodVectors.ServerFixture)allocator.allocateInstance(FoodVectors.ServerFixture.class);
    for(var mode:GameType.values())for(boolean invulnerable:List.of(false,true))for(boolean sprint:List.of(false,true)) {
      var p=(Receiver)allocator.allocateInstance(Receiver.class);var source=new TravelVectors(level);
      for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(var field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){field.setAccessible(true);field.set(p,field.get(source));}
      p.server=server;mode.updatePlayerAbilities(p.getAbilities());p.getAbilities().invulnerable=invulnerable;p.setYRot(37.25f);p.setSprinting(sprint);p.setDeltaMovement(0.03,-0.1,0.07);
      double power=sprint?0.42:0.0;p.getAttribute(Attributes.JUMP_STRENGTH).setBaseValue(power);p.jumpFromGround();
      System.out.println("server-jump\t"+mode.getId()+"\t"+invulnerable+"\t"+sprint+"\t"+TravelVectors.d(power)+"\t"+TrackingVectors.vec(p.getDeltaMovement())+"\t"+p.awarded+"\t"+FoodVectors.state(p.getFoodData()));
    }
  }
}
