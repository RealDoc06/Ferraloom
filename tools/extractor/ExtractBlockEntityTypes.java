import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.world.Nameable;

/** Pure registry facts from BlockEntityType and its registered receiver classes. */
public final class ExtractBlockEntityTypes {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
    var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(pending->pending.apply());
    var result=new JsonArray();
    for(var type:BuiltInRegistries.BLOCK_ENTITY_TYPE){
      var row=new JsonObject();row.addProperty("id",BuiltInRegistries.BLOCK_ENTITY_TYPE.getId(type));
      row.addProperty("name",BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type).toString());
      row.addProperty("only_op_can_set_nbt",type.onlyOpCanSetNbt());
      var blocks=new JsonArray();
      Boolean nameable=null;
      for(var block:BuiltInRegistries.BLOCK)if(type.isValid(block.defaultBlockState())){
        blocks.add(BuiltInRegistries.BLOCK.getId(block));
        boolean receiverNameable=type.create(BlockPos.ZERO,block.defaultBlockState()) instanceof Nameable;
        if(nameable!=null&&nameable!=receiverNameable)throw new IllegalStateException("Nameable varies by valid block: "+row.get("name"));
        nameable=receiverNameable;
      }
      if(nameable==null)throw new IllegalStateException("No valid block for "+row.get("name"));
      row.addProperty("is_nameable",nameable);
      row.add("valid_blocks",blocks);result.add(row);
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
  }
}
