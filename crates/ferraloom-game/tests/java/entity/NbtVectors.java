import java.util.*;
import net.minecraft.advancements.predicates.NbtPredicate;
import net.minecraft.core.component.DataComponents;
import net.minecraft.nbt.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;

/** Actual NbtPredicate reads, including entities excluded from persistent save. */
public final class NbtVectors {
  public static void run(Level level) {
    for(String scenario:List.of("player-empty","player-selected","player-zero","item-live","item-removed","item-riding","mob")) {
      Entity entity;
      String[] fields;
      if(scenario.startsWith("player")) {
        var player=new LivingVectors(level);entity=player;
        var stack=new ItemStack(Items.STONE,scenario.equals("player-zero")?0:3);
        if(!scenario.equals("player-empty"))player.getInventory().setItem(0,stack);
        fields=new String[]{"Inventory","SelectedItem","SelectedItemSlot","Health","abilities"};
      } else if(scenario.startsWith("item")) {
        var item=new ItemEntity(EntityTypes.ITEM,level);item.setItem(new ItemStack(Items.STONE,3));entity=item;
        item.setPickUpDelay(7);
        if(scenario.equals("item-removed"))item.discard();
        if(scenario.equals("item-riding")) {
          var vehicle=new PrimedTnt(EntityTypes.TNT,level);vehicle.setPos(10,70,-20);item.startRiding(vehicle,true,false);
        }
        fields=new String[]{"Item","Age","Health","PickupDelay"};
      } else {
        var cow=new Cow(EntityTypes.COW,level);cow.setHealth(7.5f);cow.setAge(-17);entity=cow;
        fields=new String[]{"Age","InLove","Health","HurtTime"};
      }
      entity.setId(1);entity.setUUID(new UUID(0,1));entity.setPos(1,64,2);entity.setYRot(0);entity.setXRot(0);entity.setDeltaMovement(0,0,0);
      var actual=NbtPredicate.getEntityTagToCompare(entity);var projection=new CompoundTag();
      for(String field:fields)if(actual.contains(field))projection.put(field,actual.get(field));
      for(String field:List.of("Pos","Motion","Rotation","fall_distance","Fire","Air","OnGround","Invulnerable","PortalCooldown","UUID"))projection.put(field,actual.get(field));
      System.out.println("entity-nbt\t"+scenario+"\t"+actual.contains("id")+"\t"+actual.contains("DataVersion")+"\t"+projection);
    }
  }
}
