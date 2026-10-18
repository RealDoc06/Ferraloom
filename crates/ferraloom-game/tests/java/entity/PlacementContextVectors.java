import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.EntityCollisionContext;
import net.minecraft.world.phys.shapes.Shapes;

/** Executes CollisionContext.placementContext, including the nullable receiver. */
public final class PlacementContextVectors {
  public static void run(Level level) throws Exception {
    var bottom = EntityCollisionContext.class.getDeclaredField("entityBottom");
    bottom.setAccessible(true);
    for (int scenario = 0; scenario < 8; scenario++) {
      TravelVectors player = scenario == 0 ? null : new TravelVectors(level);
      if (player != null) {
        player.setPos(8.5, scenario == 5 ? -2.5 : 64.0, 8.5);
        player.setBoundingBox(new AABB(8, 71, 8, 9, 73, 9));
        player.setShiftKeyDown(scenario == 2);
        player.getInventory().setSelectedSlot(scenario == 6 ? 5 : 0);
        player.setItemSlot(EquipmentSlot.MAINHAND,
            scenario == 4 ? ItemStack.EMPTY : new ItemStack(Items.STONE,
                scenario == 3 ? 0 : scenario == 6 ? -2 : 3));
      }
      var context = CollisionContext.placementContext(player);
      if (scenario == 7) player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.DIRT));
      var pos = new BlockPos(8, 63, 8);
      System.out.println("placement-context\t" + scenario + "\t"
          + context.isDescending() + "," + context.isPlacement() + ","
          + Long.toHexString(Double.doubleToRawLongBits(bottom.getDouble(context))) + ","
          + context.isHoldingItem(Items.AIR) + "," + context.isHoldingItem(Items.STONE) + ","
          + context.isHoldingItem(Items.DIRT) + "," + context.alwaysCollideWithFluid() + ","
          + context.canStandOnFluid(Fluids.EMPTY.defaultFluidState(), Fluids.WATER.defaultFluidState()) + ","
          + (((EntityCollisionContext)context).getEntity() == player) + ","
          + context.isAbove(Shapes.block(), pos, false) + ","
          + context.isAbove(Shapes.block(), pos, true) + ","
          + context.isAbove(Shapes.empty(), pos, false) + ","
          + context.isAbove(Shapes.empty(), pos, true));
    }
    for (int scenario = 0; scenario < 8; scenario++) {
      Entity entity = scenario < 4 ? new TravelVectors(level) : new ItemEntity(level, 8.5, 64, 8.5, new ItemStack(Items.STONE));
      entity.setPos(8.5, scenario % 2 == 0 ? 63.5 : 64.0, 8.5);
      entity.setBoundingBox(new AABB(8, 71, 8, 9, 73, 9));
      entity.setShiftKeyDown(scenario % 2 != 0);
      if (entity instanceof TravelVectors player) player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.STONE, scenario == 1 ? 3 : 0));
      var context = CollisionContext.of(entity, scenario % 3 == 0);
      System.out.println("entity-collision-context\t" + scenario + "\t"
          + Long.toHexString(Double.doubleToRawLongBits(entity.getY())) + ","
          + Long.toHexString(Double.doubleToRawLongBits(entity.getBoundingBox().minY)) + ","
          + Long.toHexString(Double.doubleToRawLongBits(bottom.getDouble(context))) + ","
          + context.isDescending() + "," + context.isPlacement() + ","
          + context.isHoldingItem(Items.AIR) + "," + context.isHoldingItem(Items.STONE) + ","
          + context.alwaysCollideWithFluid() + ","
          + context.isAbove(Shapes.block(), new BlockPos(8, 63, 8), false));
    }
  }
}
