import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.world.item.component.CustomData;

/** Observes CustomData.CODEC's CompoundTag copy, including colliding key order. */
public final class CustomDataOrderVectors {
    public static void main(String[] args) throws Exception {
        var rows = new JsonArray();
        for (String source : List.of(
            "{}",
            "{text:\"Before\",copy:12}",
            "{copy:12,text:\"Before\"}",
            "{n:4,items:[1,2],object:{a:1},text:\"Before\",changed:1b}",
            "{BB:1,Aa:2,text:\"value\",copy:12}",
            "{outer:{text:\"Before\",copy:12},list:[{object:{a:1},changed:1b}]}",
            "{AaAa:1,BBBB:2,AaBB:3,BBAa:4}",
            "{outer:{list:[1b,2b],ints:[I;1,2],longs:[L;3L],bytes:[B;4b],text:\"same\",copy:5}}"
        )) {
            var input = TagParser.parseCompoundFully(source);
            var component = CustomData.CODEC.parse(NbtOps.INSTANCE, input).getOrThrow();
            var encoded = (CompoundTag)CustomData.CODEC.encodeStart(NbtOps.INSTANCE, component).getOrThrow();
            var bytes = new ByteArrayOutputStream();
            NbtIo.writeAnyTag(encoded, new DataOutputStream(bytes));
            var keys = new JsonArray();
            encoded.keySet().forEach(keys::add);
            var row = new JsonObject();
            row.addProperty("source", source);
            row.add("keys", keys);
            row.addProperty("nbt", HexFormat.of().formatHex(bytes.toByteArray()));
            row.addProperty("json", CustomData.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow().toString());
            rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows)+"\n");
    }
}
