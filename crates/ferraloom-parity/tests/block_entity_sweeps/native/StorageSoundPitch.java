import java.lang.reflect.Method;
import java.util.*;
import com.google.gson.Gson;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.sounds.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import static org.mockito.Mockito.*;

/** Actual chest/barrel sound methods; the mock replaces only packet transport. */
public final class StorageSoundPitch {
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var sounds=new ArrayList<Map<String,Object>>();
    var level=mock(ServerLevel.class,invocation->{
      if(invocation.getMethod().getName().equals("playSeededSound")&&invocation.getArguments().length==9&&invocation.getArgument(4) instanceof Holder<?> holder){
        var row=new LinkedHashMap<String,Object>();
        row.put("key",BuiltInRegistries.SOUND_EVENT.getKey((SoundEvent)holder.value()).toString());
        row.put("position",List.of(invocation.getArgument(1),invocation.getArgument(2),invocation.getArgument(3)));
        row.put("source",invocation.getArgument(5).toString());row.put("volume",invocation.getArgument(6));row.put("pitch",invocation.getArgument(7));
        sounds.add(row);return null;
      }
      return invocation.callRealMethod();
    });
    // Run Level's actual field initializers; neither source has a fixed seed.
    var random=Level.class.getDeclaredField("random");random.setAccessible(true);random.set(level,RandomSource.create());
    var seeds=Level.class.getDeclaredField("soundSeedGenerator");seeds.setAccessible(true);seeds.set(level,RandomSource.createThreadSafe());
    var pos=new BlockPos(8,79,9);
    Method chest=ChestBlockEntity.class.getDeclaredMethod("playSound",Level.class,BlockPos.class,BlockState.class,SoundEvent.class);chest.setAccessible(true);
    chest.invoke(null,level,pos,Blocks.CHEST.defaultBlockState(),SoundEvents.CHEST_OPEN);
    chest.invoke(null,level,pos,Blocks.CHEST.defaultBlockState(),SoundEvents.CHEST_CLOSE);
    var barrel=new BarrelBlockEntity(pos,Blocks.BARREL.defaultBlockState());barrel.setLevel(level);
    Method barrelSound=BarrelBlockEntity.class.getDeclaredMethod("playSound",BlockState.class,SoundEvent.class);barrelSound.setAccessible(true);
    barrelSound.invoke(barrel,Blocks.BARREL.defaultBlockState(),SoundEvents.BARREL_OPEN);
    barrelSound.invoke(barrel,Blocks.BARREL.defaultBlockState(),SoundEvents.BARREL_CLOSE);
    System.out.println("native-storage-sounds\t"+new Gson().toJson(sounds));
  }
}
