import com.google.gson.*;
import net.minecraft.core.registries.BuiltInRegistries;
/** Immutable registry facts; contains no game-event dispatch behavior. */
public final class ExtractGameEvents {
    public static void main(String[] args) {
        var output=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(output);
        var values=new JsonArray();
        for(var event:BuiltInRegistries.GAME_EVENT) {
            var value=new JsonObject();value.addProperty("name",BuiltInRegistries.GAME_EVENT.getKey(event).toString());
            value.addProperty("notification_radius",event.notificationRadius());values.add(value);
        }
        System.out.println("facts\t"+values);
    }
}
