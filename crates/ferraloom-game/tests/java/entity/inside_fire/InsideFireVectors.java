import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.damagesource.DamageSources;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import net.minecraft.util.RandomSource;
import static org.mockito.Mockito.*;

/** Actual InsideBlockEffectType consumers and ItemEntity.fireImmune. */
public final class InsideFireVectors {
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    var registries=NeighborVectors.load();
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var provider=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(provider).forEach(pending->pending.apply());
    var level=mock(Level.class);when(level.damageSources()).thenReturn(new DamageSources(registries));
    var random=RandomSource.create(98);when(level.getRandom()).thenReturn(random);
    for(var item:List.of(Items.STONE,Items.NETHERITE_INGOT))for(int count:new int[]{-1,0,1})for(var effect:List.of(InsideBlockEffectType.FIRE_IGNITE,InsideBlockEffectType.LAVA_IGNITE))for(int fire:new int[]{-2,-1,0,301}) {
      var entity=new ItemEntity(EntityTypes.ITEM,level);entity.setItem(new ItemStack(item,count));
      entity.setRemainingFireTicks(fire);entity.setTicksFrozen(20);random.setSeed(98);
      effect.effect().accept(entity);
      System.out.println("inside-ignite\t"+BuiltInRegistries.ITEM.getKey(item)+"\t"+count+"\t"+effect.name()+"\t"+fire+"\t"+entity.getRemainingFireTicks()+"\t"+entity.getTicksFrozen()+"\t"+entity.fireImmune()+"\t"+entity.isOnFire()+"\t"+Long.toHexString(random.nextLong()));
    }
  }
}
