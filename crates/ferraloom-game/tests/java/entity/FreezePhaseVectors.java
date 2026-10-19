import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;

/** Runs the real LivingEntity.aiStep freezing phase, controlling its earlier
 * travel/effect callbacks and recording the typed damage callback after frost. */
public final class FreezePhaseVectors {
  static final class ServerFixture extends ServerLevel {
    DamageSources sources;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public DamageSources damageSources(){return sources;}
  }
  static final class Receiver extends LivingEntity {
    boolean powder,eligible;BlockState supporting;String hurt="-";
    Receiver(Level level){super(EntityTypes.PLAYER,level);}
    @Override public HumanoidArm getMainArm(){return HumanoidArm.RIGHT;}
    @Override public MoveSimulationType getMoveSimulationType(){return MoveSimulationType.AUTHORITATIVE_SIDE_AND_SERVER;}
    @Override public boolean isEffectiveAi(){return true;}
    @Override public boolean canFreeze(){return eligible;}
    @Override public void travel(Vec3 input){}
    @Override protected void applyEffectsFromBlocks(){setIsInPowderSnow(powder);}
    @Override protected void pushEntities(){}
    @Override protected BlockState getBlockStateOnLegacy(){return supporting;}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float damage){hurt=source.type().msgId()+":"+TravelVectors.f(damage)+":"+getTicksFrozen()+":"+TravelVectors.d(getAttributeValue(Attributes.MOVEMENT_SPEED));return false;}
  }
  public static void run(Level level)throws Exception {
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    for(int frozen:new int[]{Integer.MIN_VALUE,-1,0,1,139,140,141,Integer.MAX_VALUE})for(boolean powder:List.of(false,true))for(boolean eligible:List.of(false,true))for(int tick:new int[]{39,40}) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);server.sources=level.damageSources();
      var entity=new Receiver(level);entity.powder=powder;entity.eligible=eligible;entity.supporting=Blocks.STONE.defaultBlockState();entity.setTicksFrozen(frozen);entity.tickCount=tick;
      ExperienceVectors.field(Entity.class,"level").set(entity,server);
      entity.aiStep();
      var modifier=entity.getAttribute(Attributes.MOVEMENT_SPEED).getModifier(Identifier.withDefaultNamespace("powder_snow"));
      System.out.println("freeze-phase\t"+frozen+"\t"+powder+"\t"+eligible+"\t"+tick+"\t"+entity.getTicksFrozen()+"\t"+(modifier==null?"none":TravelVectors.d(modifier.amount()))+"\t"+TravelVectors.d(entity.getAttributeValue(Attributes.MOVEMENT_SPEED))+"\t"+entity.hurt);
    }
  }
}
