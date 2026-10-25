import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.EquipmentSlotGroup;

/** Observes all native slot-group predicates using the slot's wire ID. */
public final class SlotGroupVectors {
  public static void main(String[] args) {
    for(var group:EquipmentSlotGroup.values())for(var slot:EquipmentSlot.VALUES)
      System.out.println(group.ordinal()+"\t"+slot.getId()+"\t"+group.test(slot));
  }
}
