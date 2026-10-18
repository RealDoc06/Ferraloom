import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;

/** Pure BlockBehaviour#getLootTable identity, including absent and shared keys. */
public final class ExtractBlockDropTables {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var result=new JsonArray();
    for(var block:BuiltInRegistries.BLOCK)result.add(block.getLootTable().map(key->new JsonPrimitive(key.identifier().toString())).orElse(null));
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
  }
}
