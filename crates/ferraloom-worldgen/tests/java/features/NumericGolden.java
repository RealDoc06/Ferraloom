import com.google.gson.*;
import com.mojang.serialization.Codec;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
public final class NumericGolden {
    static void generate(Path out) throws Exception {
        var cases=new JsonArray();
        for(String value:List.of("0","-0.0","1.9","-1.9","4294967297","9223372036854775807","9223372036854775808","18446744073709551617","-18446744073709551617.75","1.234567890123456789e20","1e63","1e64","1e1000","1e-1000","1e2147483647","1e2147483648","1e-2147483647","1e-2147483648","true","false","\"17\"","null","[]")) {
            var input=JsonParser.parseString(value);var item=new JsonObject();item.add("input",input);
            for(String kind:List.of("int","long","float","double")) {
                try { switch(kind) {
                    case "int" -> item.addProperty(kind,Integer.toString(Codec.INT.parse(JsonOps.INSTANCE,input).getOrThrow()));
                    case "long" -> item.addProperty(kind,Long.toString(Codec.LONG.parse(JsonOps.INSTANCE,input).getOrThrow()));
                    case "float" -> item.addProperty(kind,Integer.toUnsignedString(Float.floatToRawIntBits(Codec.FLOAT.parse(JsonOps.INSTANCE,input).getOrThrow())));
                    case "double" -> item.addProperty(kind,Long.toUnsignedString(Double.doubleToRawLongBits(Codec.DOUBLE.parse(JsonOps.INSTANCE,input).getOrThrow())));
                }}catch(RuntimeException error) { item.add(kind,JsonNull.INSTANCE); }
            }
            cases.add(item);
        }
        WorldgenGolden.write(out.resolve("numeric_codecs.json.gz"),cases);
    }
}
