import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.BucketItem;
import net.minecraft.world.item.HangingSignItem;
import net.minecraft.world.item.SignApplicator;
import net.minecraft.world.level.material.Fluids;

/** Pure FeatureElement#isEnabled facts for the server's advertised VANILLA_SET. */
public final class ExtractFeatureRequirements {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
    var result=new JsonObject();var blocks=new JsonArray();var items=new JsonArray();
    for(var block:BuiltInRegistries.BLOCK)if(!block.isEnabled(FeatureFlags.VANILLA_SET))blocks.add(BuiltInRegistries.BLOCK.getId(block));
    for(var item:BuiltInRegistries.ITEM)if(!item.isEnabled(FeatureFlags.VANILLA_SET))items.add(BuiltInRegistries.ITEM.getId(item));
    result.add("disabled_blocks",blocks);result.add("disabled_items",items);
    var placements=new JsonArray();
    for(var item:BuiltInRegistries.ITEM)
      if(item instanceof BlockItem || item instanceof BucketItem bucket && bucket.getContent()!=Fluids.EMPTY)
        placements.add(BuiltInRegistries.ITEM.getId(item));
    result.add("block_placement_attempt_items",placements);
    var applicators=new JsonArray();var hangingSigns=new JsonArray();
    for(var item:BuiltInRegistries.ITEM) {
      if(item instanceof SignApplicator)applicators.add(BuiltInRegistries.ITEM.getId(item));
      if(item instanceof HangingSignItem)hangingSigns.add(BuiltInRegistries.ITEM.getId(item));
    }
    result.add("sign_applicator_items",applicators);result.add("hanging_sign_items",hangingSigns);
    var stackedOnOther=new JsonArray();var otherStackedOnMe=new JsonArray();
    for(var item:BuiltInRegistries.ITEM) {
      var type=item.getClass();
      if(type.getMethod("overrideStackedOnOther",net.minecraft.world.item.ItemStack.class,
        net.minecraft.world.inventory.Slot.class,net.minecraft.world.inventory.ClickAction.class,
        net.minecraft.world.entity.player.Player.class).getDeclaringClass()!=net.minecraft.world.item.Item.class)
        stackedOnOther.add(BuiltInRegistries.ITEM.getId(item));
      if(type.getMethod("overrideOtherStackedOnMe",net.minecraft.world.item.ItemStack.class,
        net.minecraft.world.item.ItemStack.class,net.minecraft.world.inventory.Slot.class,
        net.minecraft.world.inventory.ClickAction.class,net.minecraft.world.entity.player.Player.class,
        net.minecraft.world.entity.SlotAccess.class).getDeclaringClass()!=net.minecraft.world.item.Item.class)
        otherStackedOnMe.add(BuiltInRegistries.ITEM.getId(item));
    }
    result.add("stacked_on_other_override_items",stackedOnOther);
    result.add("other_stacked_on_me_override_items",otherStackedOnMe);
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
  }
}
