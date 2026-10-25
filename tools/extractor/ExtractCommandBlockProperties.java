import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.CommandBlock;
import net.minecraft.world.level.block.entity.CommandBlockEntity;

/** CommandBlock's constructor-supplied automatic flag, through its real factory. */
public final class ExtractCommandBlockProperties {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var rows = new JsonArray();
        for (var block : BuiltInRegistries.BLOCK) if (block instanceof CommandBlock command) {
            var entity = (CommandBlockEntity) command.newBlockEntity(BlockPos.ZERO, block.defaultBlockState());
            var row = new JsonObject();
            row.addProperty("block", BuiltInRegistries.BLOCK.getId(block));
            row.addProperty("automatic", entity.isAutomatic()); rows.add(row);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(rows) + "\n");
    }
}
