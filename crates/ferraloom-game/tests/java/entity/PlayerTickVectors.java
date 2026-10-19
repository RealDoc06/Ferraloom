import java.lang.reflect.*;
import java.util.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Observes the real listener tickPlayer frame and loaded-neighbor predicate. */
public final class PlayerTickVectors {
  static final class Receiver extends ServerPlayer {
    String called;boolean retarget;
    Receiver(){super(null,null,null,null);}
    @Override public void doTick() {
      try {
        called=ExperienceVectors.field(ServerGamePacketListenerImpl.class,"tickCount").getInt(connection)+":"+TrackingVectors.vec(new Vec3(xo,yo,zo));
        setPos(11,70,12);setYRot(retarget?-400:431);setXRot(retarget?100:-120);
        if(retarget)for(String axis:List.of("X","Y","Z"))ExperienceVectors.field(ServerGamePacketListenerImpl.class,"firstGood"+axis).setDouble(connection,axis.equals("Y")?80:axis.equals("X")?4e7:-4e7);
      } catch(Exception error){throw new AssertionError(error);}
    }
  }
  public static void run(Level level)throws Exception {
    for(double x:new double[]{-32,-31,-30.7,-30.6999,-30,0,45,45.6999,45.7,46,46.7,47,48}) {
      var player=new LivingVectors(level);player.setPos(x,64,8.5);
      System.out.println("player-loaded-neighbors\t"+Long.toHexString(Double.doubleToRawLongBits(x))+"\t"+player.touchingUnloadedChunk());
    }
    var allocatorField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");allocatorField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)allocatorField.get(null);
    var tick=ServerGamePacketListenerImpl.class.getDeclaredMethod("tickPlayer");tick.setAccessible(true);
    for(boolean retarget:List.of(false,true)) {
      var player=(Receiver)allocator.allocateInstance(Receiver.class);
      var source=new LivingVectors(level);
      for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(Field field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())) {
        field.setAccessible(true);field.set(player,field.get(source));
      }
      var server=(ExperienceVectors.ServerFixture)allocator.allocateInstance(ExperienceVectors.ServerFixture.class);
      ExperienceVectors.field(Entity.class,"level").set(player,server);
      var listener=(ServerGamePacketListenerImpl)allocator.allocateInstance(ServerGamePacketListenerImpl.class);
      ExperienceVectors.field(ServerGamePacketListenerImpl.class,"player").set(listener,player);
      ExperienceVectors.field(ServerGamePacketListenerImpl.class,"tickCount").setInt(listener,9);
      player.connection=listener;player.retarget=retarget;player.setPos(8.5,64,8.5);player.setDeltaMovement(0.25,-0.5,0.125);
      tick.invoke(listener);
      System.out.println("player-connection-frame\t"+retarget+"\t"+player.called+"\t"+TrackingVectors.vec(player.position())+"\t"+
        TrackingVectors.vec(new Vec3(player.xo,player.yo,player.zo))+"\t"+SoundVectors.f(player.getYRot())+","+SoundVectors.f(player.getXRot())+","+SoundVectors.f(player.yRotO)+","+SoundVectors.f(player.xRotO)+"\t"+
        TrackingVectors.vec(player.getDeltaMovement())+"\t"+ExperienceVectors.field(ServerGamePacketListenerImpl.class,"tickCount").getInt(listener));
    }
  }
}
