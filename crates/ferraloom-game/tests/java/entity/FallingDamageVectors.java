import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.core.Direction;
import static org.mockito.Mockito.*;

/** Actual FallingBlockEntity.causeFallDamage and Level entity query. Only the
 * victims' hurtServer bodies are observation callbacks rather than damage. */
public final class FallingDamageVectors {
  static final List<String> calls=new ArrayList<>();
  static FallingBlockEntity falling;
  static boolean changeBlock;
  static boolean hurt(Entity entity,DamageSource source,float amount) {
    calls.add(entity.getId()+":"+source.typeHolder().unwrapKey().orElseThrow().identifier()+":"+FallDamageVectors.f(amount)+":"+source.getDirectEntity().getId()+":"+source.getEntity().getId());
    if(changeBlock && entity.getId()==1) {
      try {ExperienceVectors.field(FallingBlockEntity.class,"blockState").set(falling,Blocks.STONE.defaultBlockState());}
      catch(Exception error){throw new RuntimeException(error);}
    }
    return false;
  }
  static final class CowReceiver extends Cow {
    CowReceiver(Level level){super(EntityTypes.COW,level);}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount){return FallingDamageVectors.hurt(this,source,amount);}
  }
  static final class PlayerReceiver extends Player {
    GameType mode=GameType.SURVIVAL;
    PlayerReceiver(Level level,int id){super(level,new GameProfile(new UUID(0,id),"falling"+id));}
    @Override public GameType gameMode(){return mode;}
    @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount){return FallingDamageVectors.hurt(this,source,amount);}
  }
  public static void run(Level level)throws Exception {
    var cls=level.getClass();var add=cls.getDeclaredMethod("addContact",Entity.class);add.setAccessible(true);
    var clear=cls.getDeclaredMethod("clearContacts");clear.setAccessible(true);
    var last=cls.getDeclaredField("lastContactBounds");last.setAccessible(true);
    var server=mock(ServerLevel.class);
    var state=ExperienceVectors.field(FallingBlockEntity.class,"blockState");
    var cancel=ExperienceVectors.field(FallingBlockEntity.class,"cancelDrop");
    double[] distances={Double.NEGATIVE_INFINITY,-1.00001,-1,-0.0,0.5,1,1.00001,2,2.5,4,9,20,Double.POSITIVE_INFINITY,Double.NaN};
    float[] amounts={0,0.125F,0.25F,2,5,-1,Float.POSITIVE_INFINITY,Float.NaN};
    int[] limits={-5,0,1,40,Integer.MAX_VALUE};
    long[] seeds={-1,0,1,12345,Long.MAX_VALUE};
    Block[] blocks={Blocks.SAND,Blocks.STONE,Blocks.ANVIL,Blocks.CHIPPED_ANVIL,Blocks.DAMAGED_ANVIL};
    for(int i=0;i<112;i++) {
      clear.invoke(level);
      var block=blocks[i%blocks.length].defaultBlockState();
      if(block.hasProperty(AnvilBlock.FACING))block=block.setValue(AnvilBlock.FACING,Direction.from2DDataValue(i%4));
      double distance=distances[i%distances.length];float amount=amounts[(i/3)%amounts.length];int limit=limits[(i/11)%limits.length];long seed=seeds[(i/7)%seeds.length];boolean hurts=i%7!=0;
      falling=new FallingBlockEntity(EntityTypes.FALLING_BLOCK,level);falling.setId(77);falling.setPos(8.5,64,8.5);state.set(falling,block);
      if(hurts)falling.setHurtsEntities(amount,limit);
      falling.getRandom().setSeed(seed);add.invoke(level,falling);
      for(int id=1;id<=9;id++) {
        Entity target;
        if(id>=2 && id<=4) {
          var player=new PlayerReceiver(level,id);player.mode=id==2?GameType.SURVIVAL:id==3?GameType.CREATIVE:GameType.SPECTATOR;
          // Selection uses game mode, independently of mutable abilities.
          player.getAbilities().instabuild=id==2;player.getAbilities().invulnerable=id==2;target=player;
        } else if(id==7)target=new ItemEntity(EntityTypes.ITEM,level);
        else target=new CowReceiver(level);
        target.setId(id);target.setPos(id==8?12.0:8.5,64,8.5);
        if(id==5)((LivingEntity)target).setHealth(0);
        if(id==6)target.discard();
        ExperienceVectors.field(Entity.class,"level").set(target,server);add.invoke(level,target);
      }
      calls.clear();changeBlock=i%17==0;
      boolean result=falling.causeFallDamage(distance,9,level.damageSources().drown());
      System.out.println("falling-damage\t"+Block.getId(block)+"\t"+FallDamageVectors.d(distance)+"\t"+FallDamageVectors.f(amount)+"\t"+limit+"\t"+seed+"\t"+hurts+"\t"+changeBlock+"\t"+result+"\t"+Block.getId(falling.getBlockState())+"\t"+cancel.getBoolean(falling)+"\t"+(calls.isEmpty()?"-":String.join(";",calls))+"\t"+Long.toHexString(falling.getRandom().nextLong())+"\t"+(last.get(level)!=null));
    }
    clear.invoke(level);
  }
}
