import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

/** Reflects the actual declaring method over the whole native item registry. */
public final class ItemTickVectors {
  public static void run()throws Exception {
    for(var item:BuiltInRegistries.ITEM) {
      var owner=item.getClass().getMethod("inventoryTick",ItemStack.class,ServerLevel.class,Entity.class,EquipmentSlot.class).getDeclaringClass();
      if(owner!=Item.class)System.out.println("item-inventory-tick-override\t"+BuiltInRegistries.ITEM.getKey(item)+"\t"+owner.getSimpleName()+"#inventoryTick");
    }
  }
}
