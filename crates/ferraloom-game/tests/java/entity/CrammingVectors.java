import java.util.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.phys.AABB;

/** Real LivingEntity push loop with a controlled captured query and hurt callback. */
public final class CrammingVectors {
  static final class ServerFixture extends ServerLevel {
    List<Entity> selected;GameRules rules;DamageSources sources;int queries;
    net.minecraft.world.TickRateManager tickRate;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public List<Entity> getPushableEntities(Entity source,AABB bounds){queries++;return selected;}
    @Override public GameRules getGameRules(){return rules;}
    @Override public DamageSources damageSources(){return sources;}
    @Override public net.minecraft.world.TickRateManager tickRateManager(){return tickRate;}
  }
  static final class Receiver extends Cow {
    ServerFixture server;final StringJoiner events=new StringJoiner(",");
    Receiver(Level level){super(EntityTypes.COW,level);}
    @Override public Level level(){return server==null?super.level():server;}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount) {
      events.add("hurt:"+source.type().msgId()+":"+Integer.toHexString(Float.floatToRawIntBits(amount)));
      server.selected.getLast().discard();
      return false;
    }
    @Override protected void doPush(Entity other){events.add("push:"+other.getId()+":"+other.isRemoved());}
    void runPush(){super.pushEntities();}
  }
  public static void run(Level level)throws Exception {
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);
    var allocator=(sun.misc.Unsafe)field.get(null);
    int[][] setups={{0,1,0,4096},{1,0,0,4096},{1,1,0,4096},{1,1,1,4096},
      {1,2,0,4096},{3,2,0,4096},{3,2,1,4096},{3,2,2,4096},
      {3,3,0,4096},{3,3,1,4096},{3,4,0,4096},{3,1,0,0},
      {3,1,0,4096},{3,1,0,-1},{3,1,2,4096}};
    for(int[] setup:setups) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);
      server.rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);server.sources=level.damageSources();
      server.rules.set(GameRules.MAX_ENTITY_CRAMMING,setup[1],null);
      var receiver=new Receiver(level);var targets=new ArrayList<Entity>();
      var mount=new PrimedTnt(EntityTypes.TNT,level);
      for(int i=0;i<setup[0];i++) {
        var target=new PrimedTnt(EntityTypes.TNT,level);target.setId(100+i);targets.add(target);
        if(setup[2]==1||setup[2]==2&&i>0)target.startRiding(mount,true,false);
      }
      server.selected=List.copyOf(targets);receiver.server=server;receiver.getRandom().setSeed(setup[3]);
      receiver.runPush();
      System.out.println("living-cramming\t"+setup[0]+"\t"+setup[1]+"\t"+setup[2]+"\t"+setup[3]+"\t"+
        receiver.events+"\t"+receiver.getRandom().nextLong()+"\t"+server.queries);
    }
    var method=net.minecraft.server.level.ServerPlayer.class.getDeclaredMethod("pushEntities");method.setAccessible(true);
    var levelField=Entity.class.getDeclaredField("level");levelField.setAccessible(true);
    for(int stage=0;stage<3;stage++) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);
      server.selected=List.of();server.tickRate=new net.minecraft.world.TickRateManager();
      if(stage>0)server.tickRate.setFrozen(true);
      if(stage==2)server.tickRate.tick();
      var player=(net.minecraft.server.level.ServerPlayer)allocator.allocateInstance(net.minecraft.server.level.ServerPlayer.class);
      levelField.set(player,server);method.invoke(player);
      System.out.println("living-push-rate\t"+stage+"\t"+server.tickRate.runsNormally()+"\t"+server.queries);
    }
  }
}
