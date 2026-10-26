import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;

/** Native WeatheringCopper membership/age and the single NEXT_BY_BLOCK map.
 * PREVIOUS and FIRST are derived from this map by the Rust data generator. */
public final class ExtractWeatheringCopper {
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var result=new JsonObject();var blocks=new JsonArray();
        result.addProperty("block_count",BuiltInRegistries.BLOCK.size());
        for(var block:BuiltInRegistries.BLOCK) {
            if(block instanceof ChangeOverTimeBlock<?> aging
                && aging.getAge().getDeclaringClass().equals(WeatheringCopper.WeatherState.class)
                && !(block instanceof WeatheringCopper))
                throw new IllegalStateException("unmodeled same-enum aging implementation: "+BuiltInRegistries.BLOCK.getKey(block));
            if(!(block instanceof WeatheringCopper copper))continue;
            var row=new JsonObject();row.addProperty("block",BuiltInRegistries.BLOCK.getId(block));
            row.addProperty("age",copper.getAge().ordinal());
            var next=WeatheringCopper.NEXT_BY_BLOCK.get().get(block);
            if(next==null)row.add("next",JsonNull.INSTANCE);
            else row.addProperty("next",BuiltInRegistries.BLOCK.getId(next));
            blocks.add(row);
        }
        result.add("blocks",blocks);
        Files.writeString(Path.of(args[0]),new GsonBuilder().serializeNulls().setPrettyPrinting().create().toJson(result)+"\n");
    }
}
