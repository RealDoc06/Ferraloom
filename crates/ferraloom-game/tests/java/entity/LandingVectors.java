import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.particles.BlockParticleOption;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import org.mockito.Answers;
import static org.mockito.Mockito.*;

/** Actual LivingEntity.checkFallDamage with controlled onChangedBlock/damage.
 * The real ServerLevel particle and game-event calls are captured in order. */
public final class LandingVectors {
  static final class Receiver extends Cow {
    Receiver(Level level){super(EntityTypes.COW,level);}
    @Override protected void onChangedBlock(ServerLevel level,BlockPos pos){gameEvent(GameEvent.STEP);}
    @Override public boolean causeFallDamage(double distance,float modifier,DamageSource source){return false;}
    void check(double dy,boolean ground,BlockState state,BlockPos pos){super.checkFallDamage(dy,ground,state,pos);}
    boolean climbableCleared() throws Exception {return ((Optional<?>)ExperienceVectors.field(LivingEntity.class,"lastClimbablePos").get(this)).isEmpty();}
  }
  public static void run(Level original) throws Exception {
    for(int scenario=0;scenario<12;scenario++) {
      var events=new ArrayList<String>();
      var server=mock(ServerLevel.class, call->{
        var a=call.getArguments();
        if(call.getMethod().getName().equals("damageSources"))return original.damageSources();
        if(call.getMethod().getName().equals("sendParticles")) {
          events.add("particles:"+Block.getId(((BlockParticleOption)a[0]).getState())+":"+TravelVectors.d((double)a[1])+","+TravelVectors.d((double)a[2])+","+TravelVectors.d((double)a[3])+":"+a[4]+":"+TravelVectors.d((double)a[5])+","+TravelVectors.d((double)a[6])+","+TravelVectors.d((double)a[7])+":"+TravelVectors.d((double)a[8]));return 0;
        }
        if(call.getMethod().getName().equals("gameEvent")) {
          Holder<?> holder=(Holder<?>)(a[0] instanceof Holder<?>?a[0]:a[1]);
          events.add("event:"+BuiltInRegistries.GAME_EVENT.getKey((GameEvent)holder.value()));return null;
        }
        return Answers.RETURNS_DEFAULTS.answer(call);
      });
      var entity=new Receiver(original);entity.setPos(8.5,64.25,8.5);
      ExperienceVectors.field(Entity.class,"level").set(entity,server);
      ExperienceVectors.field(Entity.class,"wasTouchingWater").setBoolean(entity,true);
      ExperienceVectors.field(LivingEntity.class,"lastClimbablePos").set(entity,Optional.of(new BlockPos(8,64,8)));
      entity.fallDistance=new double[]{0,0.25,3.25,7.75,100,-1,0,7.75,7.75,3.25,100,4}[scenario];
      boolean ground=scenario%3!=0;
      var state=(scenario%4==0?Blocks.AIR:Blocks.STONE).defaultBlockState();
      var pos=new BlockPos(scenario%2==0?8:7,63,scenario%2==0?8:7);
      entity.check(-0.125,ground,state,pos);
      System.out.println("living-landing\t"+scenario+"\t"+TravelVectors.d(entity.fallDistance)+"\t"+entity.climbableCleared()+"\t"+(events.isEmpty()?"-":String.join(";",events)));
    }
  }
}
