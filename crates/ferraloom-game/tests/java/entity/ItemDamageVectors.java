import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.tags.DamageTypeTags;
import net.minecraft.util.Unit;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.DamageResistant;
import net.minecraft.world.level.Level;

/** Actual effective component queries, including retained zero/negative counts. */
public final class ItemDamageVectors {
  public static void run(Level level){
    for(var item:List.of(Items.STONE,Items.IRON_SWORD,Items.ELYTRA,Items.NETHERITE_INGOT))for(String mode:List.of("prototype","remove-damage","force-damage","unbreakable"))for(int count:new int[]{-1,0,1}){
      var stack=new ItemStack(item);
      switch(mode){
        case "remove-damage" -> stack.remove(DataComponents.DAMAGE);
        case "force-damage" -> {stack.set(DataComponents.MAX_DAMAGE,17);stack.set(DataComponents.DAMAGE,0);}
        case "unbreakable" -> stack.set(DataComponents.UNBREAKABLE,Unit.INSTANCE);
      }
      stack.setCount(count);
      System.out.println("item-damageable\t"+BuiltInRegistries.ITEM.getKey(item)+"\t"+mode+"\t"+count+"\t"+stack.isDamageableItem());
    }
    var sources=List.of(level.damageSources().inFire(),level.damageSources().onFire(),level.damageSources().lava(),level.damageSources().fall(),level.damageSources().generic());
    for(String mode:List.of("none","prototype","direct-fire","direct-mixed","direct-empty","tag-fire","tag-fall"))for(int count:new int[]{-1,0,1}){
      var stack=new ItemStack(mode.equals("prototype")?Items.NETHERITE_INGOT:Items.STONE);
      switch(mode){
        case "direct-fire" -> stack.set(DataComponents.DAMAGE_RESISTANT,new DamageResistant(HolderSet.direct(sources.get(0).typeHolder())));
        case "direct-mixed" -> stack.set(DataComponents.DAMAGE_RESISTANT,new DamageResistant(HolderSet.direct(sources.get(0).typeHolder(),sources.get(3).typeHolder())));
        case "direct-empty" -> stack.set(DataComponents.DAMAGE_RESISTANT,new DamageResistant(HolderSet.direct(List.of())));
        case "tag-fire" -> stack.set(DataComponents.DAMAGE_RESISTANT,new DamageResistant(level.registryAccess().lookupOrThrow(net.minecraft.core.registries.Registries.DAMAGE_TYPE).getOrThrow(DamageTypeTags.IS_FIRE)));
        case "tag-fall" -> stack.set(DataComponents.DAMAGE_RESISTANT,new DamageResistant(level.registryAccess().lookupOrThrow(net.minecraft.core.registries.Registries.DAMAGE_TYPE).getOrThrow(DamageTypeTags.IS_FALL)));
      }
      stack.setCount(count);
      for(var source:sources)System.out.println("item-damage-resistant\t"+mode+"\t"+count+"\t"+source.typeHolder().unwrapKey().orElseThrow().identifier()+"\t"+stack.canBeHurtBy(source));
    }
  }
}
