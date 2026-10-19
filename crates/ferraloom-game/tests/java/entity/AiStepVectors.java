import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.AABB;

/** Real Player.aiStep order with controlled travel, inside-block and push
 * callbacks. Jump, damping, input, frost and Player's tail execute natively. */
public final class AiStepVectors {
  static final class ServerFixture extends ServerLevel {
    Level source;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public net.minecraft.world.level.block.state.BlockState getBlockState(BlockPos pos){return source.getBlockState(pos);}
    @Override public List<Entity> getEntities(Entity except,AABB box,java.util.function.Predicate<? super Entity> predicate){return List.of();}
  }
  static final class Receiver extends Player {
    final StringJoiner events=new StringJoiner(";");
    Receiver(Level level){super(level,new GameProfile(new UUID(3,4),"ai"));}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public boolean canFreeze(){return false;}
    @Override protected net.minecraft.world.level.block.state.BlockState getBlockStateOnLegacy(){return Blocks.STONE.defaultBlockState();}
    @Override public void jumpFromGround(){events.add("jump");super.jumpFromGround();}
    @Override public void travel(Vec3 input){events.add("travel:"+TrackingVectors.vec(input)+":"+TrackingVectors.vec(getDeltaMovement())+":"+TravelVectors.d(fallDistance)+":"+delay());}
    @Override protected void applyEffectsFromBlocks(){events.add("blocks");setTicksFrozen(42);}
    @Override protected void pushEntities(){events.add("push:"+getTicksFrozen()+":"+TravelVectors.d(getAttributeValue(Attributes.MOVEMENT_SPEED)));}
    int delay(){try{return ExperienceVectors.field(LivingEntity.class,"noJumpDelay").getInt(this);}catch(Exception ex){throw new AssertionError(ex);}}
    float storedSpeed(){try{return ExperienceVectors.field(LivingEntity.class,"speed").getFloat(this);}catch(Exception ex){throw new AssertionError(ex);}}
    String result(){return TrackingVectors.vec(getDeltaMovement())+"\t"+TravelVectors.f(xxa)+","+TravelVectors.f(yya)+","+TravelVectors.f(zza)+"\t"+jumping+"\t"+delay()+"\t"+jumpTriggerTime+"\t"+TravelVectors.f(yHeadRot)+"\t"+TravelVectors.f(storedSpeed())+"\t"+TravelVectors.d(fallDistance)+"\t"+events;}
  }
  public static void run(Level level)throws Exception {
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);var allocator=(sun.misc.Unsafe)field.get(null);
    var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);server.source=level;
    level.getClass().getDeclaredMethod("floor").invoke(level);
    double[][] velocities={{0.002,0.0029,0.002},{0.003,0.003,0},{-0.0,-0.0,-0.0},{Math.nextDown(0.003),Math.nextDown(-0.003),0}};
    for(int i=0;i<32;i++) {
      var p=new Receiver(level);p.setPos(8.5,64,8.5);p.setYRot(37.25f);
      var v=velocities[i%velocities.length];p.setDeltaMovement(v[0],v[1],v[2]);p.xxa=0.75f;p.yya=-0.125f;p.zza=-0.3f;
      ExperienceVectors.field(LivingEntity.class,"jumping").setBoolean(p,i%3==0);p.setOnGround(i%2==0);ExperienceVectors.field(LivingEntity.class,"noJumpDelay").setInt(p,new int[]{-1,0,1,10}[i%4]);ExperienceVectors.field(Player.class,"jumpTriggerTime").setInt(p,2);
      p.getAbilities().flying=i%7==0;p.setHealth(i%13==0?0:20);
      if(i%11==0)p.setSleepingPos(new BlockPos(8,64,8));
      p.fallDistance=5.0;String before=p.isJumping()+"\t"+p.onGround()+"\t"+p.delay()+"\t"+p.getAbilities().flying+"\t"+(i%13==0)+"\t"+(i%11==0)+"\t"+TrackingVectors.vec(p.getDeltaMovement());
      ExperienceVectors.field(Entity.class,"level").set(p,server);
      p.aiStep();System.out.println("player-ai-step\t"+i+"\t"+before+"\t"+p.result());
    }
  }
}
