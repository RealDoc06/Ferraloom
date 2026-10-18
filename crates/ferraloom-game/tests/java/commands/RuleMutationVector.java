import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.*;
import static org.mockito.Mockito.*;

/** Distinguishes API mutation from the argument/codec range validation boundaries. */
public final class RuleMutationVector {
    static <T> void capture(JsonArray out,GameRule<T> rule,T value) {
        var rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);var server=mock(MinecraftServer.class);rules.set(rule,value,server);
        var row=new JsonObject();row.addProperty("rule",rule.getIdentifierWithFallback().toString());row.add("requested",new Gson().toJsonTree(value));
        boolean present=rules.availableRules().anyMatch(available->available==rule);row.addProperty("present",present);
        row.addProperty("notifications",mockingDetails(server).getInvocations().stream().filter(call->call.getMethod().getName().equals("onGameRuleChanged")).count());
        var map=GameRuleMap.of();if(present) {row.add("value",new Gson().toJsonTree(rules.get(rule)));map.set(rule,rules.get(rule));}
        var encoded=GameRuleMap.CODEC.encodeStart(JsonOps.INSTANCE,map);encoded.error().ifPresent(error->row.addProperty("encode_error",error.message()));row.addProperty("encoded",encoded.result().isPresent());out.add(row);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var out=new JsonArray();
        capture(out,GameRules.MAX_SNOW_ACCUMULATION_HEIGHT,9);capture(out,GameRules.RANDOM_TICK_SPEED,-1);capture(out,GameRules.RANDOM_TICK_SPEED,0);capture(out,GameRules.MAX_BLOCK_MODIFICATIONS,0);capture(out,GameRules.MAX_COMMAND_FORKS,-1);capture(out,GameRules.KEEP_INVENTORY,false);capture(out,GameRules.KEEP_INVENTORY,true);capture(out,GameRules.MAX_MINECART_SPEED,10);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
