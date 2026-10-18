import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.flag.FeatureFlags;

/** Pure FeatureElement#isEnabled facts for the server's advertised VANILLA_SET. */
public final class ExtractFeatureRequirements {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
    var result=new JsonObject();var blocks=new JsonArray();var items=new JsonArray();
    for(var block:BuiltInRegistries.BLOCK)if(!block.isEnabled(FeatureFlags.VANILLA_SET))blocks.add(BuiltInRegistries.BLOCK.getId(block));
    for(var item:BuiltInRegistries.ITEM)if(!item.isEnabled(FeatureFlags.VANILLA_SET))items.add(BuiltInRegistries.ITEM.getId(item));
    result.add("disabled_blocks",blocks);result.add("disabled_items",items);
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
  }
}
