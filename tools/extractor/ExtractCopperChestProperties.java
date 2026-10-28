import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.CopperChestBlock;

/** Immutable CopperChestBlock constructor facts; copper_wax owns wax mappings. */
public final class ExtractCopperChestProperties {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var rows = new JsonArray();
        for (var block : BuiltInRegistries.BLOCK) {
            if (!(block instanceof CopperChestBlock chest)) continue;
            var row = new JsonObject();
            row.addProperty("block", BuiltInRegistries.BLOCK.getId(block));
            row.addProperty("weathering_stage", chest.getState().ordinal());
            row.addProperty("is_waxed", chest.isWaxed());
            rows.add(row);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(rows) + "\n");
    }
}
