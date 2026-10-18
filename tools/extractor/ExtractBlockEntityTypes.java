import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;

/** Pure registry facts from BlockEntityType#isValid and #onlyOpCanSetNbt. */
public final class ExtractBlockEntityTypes {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
    var result=new JsonArray();
    for(var type:BuiltInRegistries.BLOCK_ENTITY_TYPE){
      var row=new JsonObject();row.addProperty("id",BuiltInRegistries.BLOCK_ENTITY_TYPE.getId(type));
      row.addProperty("name",BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type).toString());
      row.addProperty("only_op_can_set_nbt",type.onlyOpCanSetNbt());
      var blocks=new JsonArray();
      for(var block:BuiltInRegistries.BLOCK)if(type.isValid(block.defaultBlockState()))blocks.add(BuiltInRegistries.BLOCK.getId(block));
      row.add("valid_blocks",blocks);result.add(row);
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
  }
}
