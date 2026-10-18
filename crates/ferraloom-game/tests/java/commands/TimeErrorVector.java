import com.google.gson.*;
import com.mojang.brigadier.exceptions.Dynamic2CommandExceptionType;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.TimeCommand;

/** Native TimeCommand exception suppliers retain the numeric argument type. */
public final class TimeErrorVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var vectors = new JsonArray();
        for (String field : new String[]{"ERROR_ALREADY_AT_TIME","ERROR_ALREADY_SAME_RATE"}) {
            var f=TimeCommand.class.getDeclaredField(field); f.setAccessible(true);
            var type=(Dynamic2CommandExceptionType)f.get(null);
            var values=field.equals("ERROR_ALREADY_AT_TIME") ? new Number[]{0,1000} : new Number[]{0.5f,2.0f,3.25f};
            for (var value:values) {
                var error=type.create("minecraft:overworld",value);
                var row=new JsonObject();
                row.addProperty("command", "time "+(field.equals("ERROR_ALREADY_AT_TIME") ? "set " : "rate ")+value);
                row.addProperty("value",value);
                row.addProperty("rate",field.equals("ERROR_ALREADY_SAME_RATE"));
                row.addProperty("message",error.getRawMessage().getString());
                row.add("component",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,(Component)error.getRawMessage()).getOrThrow());
                vectors.add(row);
            }
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(vectors)+"\n");
    }
}
