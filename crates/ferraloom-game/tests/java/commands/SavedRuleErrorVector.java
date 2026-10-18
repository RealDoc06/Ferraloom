import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.gamerules.GameRuleMap;

/** Error accumulation follows CompoundTag's JDK entry traversal, not key sorting. */
public final class SavedRuleErrorVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var out=new JsonArray();
        for(String input:List.of(
            "{'z:missing':true,'a:missing':false,'minecraft:random_tick_speed':-1,'minecraft:max_snow_accumulation_height':9}",
            "{z:1,a:1,b:1,c:1,d:1,e:1,f:1,g:1,h:1,i:1,j:1,k:1,l:1,m:1,n:1,keep_inventory:true}",
            "{Aa:1,BB:1,AaAa:1,BBBB:1,AaBB:1,BBAa:1,'minecraft:random_tick_speed':-1}",
            "{keep_inventory:'bad',random_tick_speed:-1,max_block_modifications:0,max_snow_accumulation_height:9}"
        )) {
            var result=GameRuleMap.CODEC.parse(NbtOps.INSTANCE,TagParser.create(NbtOps.INSTANCE).parseFully(input));var row=new JsonObject();row.addProperty("input",input);result.error().ifPresent(error->row.addProperty("error",error.message()));result.resultOrPartial(error->{}).ifPresent(value->row.add("partial",GameRuleMap.CODEC.encodeStart(JsonOps.INSTANCE,value).getOrThrow()));out.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
