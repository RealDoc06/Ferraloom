import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;

/** Native ObjectiveCriteria metadata, including each kind of Stat. */
public final class CriteriaVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var names = new TreeSet<>(ObjectiveCriteria.getCustomCriteriaNames());
        names.addAll(List.of(
            "minecraft.mined:minecraft.stone", "minecraft.crafted:minecraft.stick",
            "minecraft.used:minecraft.stick", "minecraft.broken:minecraft.wooden_pickaxe",
            "minecraft.picked_up:minecraft.stone", "minecraft.dropped:minecraft.stone",
            "minecraft.killed:minecraft.zombie", "minecraft.killed_by:minecraft.zombie",
            "minecraft.custom:minecraft.jump", "mined:stone", "minecraft.mined:stone", "mined:minecraft.stone"));
        var vectors = new JsonArray();
        for (var name : names) {
            var criteria = ObjectiveCriteria.byName(name).orElseThrow();
            var vector = new JsonObject();
            vector.addProperty("input", name);
            vector.addProperty("name", criteria.getName());
            vector.addProperty("read_only", criteria.isReadOnly());
            vector.addProperty("hearts", criteria.getDefaultRenderType() == ObjectiveCriteria.RenderType.HEARTS);
            vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
