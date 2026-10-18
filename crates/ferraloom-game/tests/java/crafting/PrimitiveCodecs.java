import com.google.gson.*;
import com.mojang.serialization.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.component.*;

/** Observations of DFU codecs with Gson lazy numbers and all NBT numeric kinds. */
public final class PrimitiveCodecs {
    static final Gson JSON = new GsonBuilder().disableHtmlEscaping().create();
    static final Map<String,Codec<?>> CODECS = new LinkedHashMap<>();
    static {
        CODECS.put("int", Codec.INT); CODECS.put("long", Codec.LONG);
        CODECS.put("float", Codec.FLOAT); CODECS.put("double", Codec.DOUBLE);
        CODECS.put("bool", Codec.BOOL); CODECS.put("string", Codec.STRING);
    }
    static <T> JsonObject values(DynamicOps<T> ops, T input) {
        var result = new JsonObject();
        CODECS.forEach((name,codec) -> {
            try {
                Object value = codec.parse(ops,input).getOrThrow();
                String text = value instanceof Float n ? Integer.toHexString(Float.floatToIntBits(n))
                    : value instanceof Double n ? Long.toHexString(Double.doubleToLongBits(n)) : value.toString();
                result.addProperty(name,text);
            } catch (Exception e) { result.add(name,JsonNull.INSTANCE); }
        });
        return result;
    }
    static <T> String compound(DynamicOps<T> ops, T input) {
        try { return CustomData.CODEC.parse(ops,input).getOrThrow().copyTag().toString(); }
        catch (Exception e) { return null; }
    }
    static <T> String typed(DynamicOps<T> ops, T input) {
        try { return TypedEntityData.codec(Identifier.CODEC).parse(ops,input).getOrThrow().type().toString(); }
        catch (Exception e) { return null; }
    }
    static <T,V> String normalized(Codec<V> codec, DynamicOps<T> ops, T input) {
        try { return codec.encodeStart(NbtOps.INSTANCE,codec.parse(ops,input).getOrThrow()).getOrThrow().toString(); }
        catch (Exception e) { return null; }
    }
    public static void main(String[] args) throws Exception {
        var root = new JsonObject(); var json = new JsonArray(); var nbt = new JsonArray(); var compounds = new JsonArray();
        var inputs = new ArrayList<>(List.of("null","true","false","\"1\"","\"hello\"","[]","{}","0","-0.0","2.9","-2.9",
            "2147483648","4294967297","9223372036854775808","18446744073709551617","-18446744073709551617.9",
            "1.0000000596046447753906250000000000000001", "0.99999999999999999999999999999999", "1e9999","1e10000","1e-9999","1e-10000",
            "1e2147483648","1e-2147483649","0e999999999999999999999999999999999", "10e10000","10e-9999",
            "9223372036854775807.0","-9223372036854775808.0", "4.9406564584124654e-324", "-1e-9999"));
        inputs.add("9".repeat(10000)); inputs.add("9".repeat(10001));
        inputs.add("0."+"0".repeat(9997)+"1"); inputs.add("0."+"0".repeat(9998)+"1");
        for (String input : inputs) {
            var row = new JsonObject(); row.addProperty("input",input);
            var parsed=JsonParser.parseString(input); row.add("values",values(JsonOps.INSTANCE,parsed)); json.add(row);
        }
        var numeric = new LinkedHashMap<String,Tag>();
        numeric.put("byte",ByteTag.valueOf((byte)-1)); numeric.put("short",ShortTag.valueOf((short)257));
        numeric.put("int",IntTag.valueOf(Integer.MIN_VALUE)); numeric.put("long",LongTag.valueOf(4294967297L));
        numeric.put("longMax",LongTag.valueOf(Long.MAX_VALUE)); numeric.put("float",FloatTag.valueOf(-2.9f));
        numeric.put("double",DoubleTag.valueOf(-2.9)); numeric.put("doubleMax",DoubleTag.valueOf(Double.MAX_VALUE));
        numeric.put("negativeMax",DoubleTag.valueOf(-Double.MAX_VALUE)); numeric.put("nan",DoubleTag.valueOf(Double.NaN));
        numeric.put("floatNan",FloatTag.valueOf(Float.NaN)); numeric.put("positiveInfinity",FloatTag.valueOf(Float.POSITIVE_INFINITY));
        numeric.put("negativeInfinity",DoubleTag.valueOf(Double.NEGATIVE_INFINITY)); numeric.put("negativeZero",DoubleTag.valueOf(-0.0));
        numeric.put("fraction",DoubleTag.valueOf(0.5)); numeric.put("string",StringTag.valueOf("1"));
        numeric.put("compound",new CompoundTag()); numeric.put("list",new ListTag());
        numeric.forEach((name,input) -> { var row=new JsonObject(); row.addProperty("name",name);row.addProperty("input",input.toString());row.add("values",values(NbtOps.INSTANCE,input));nbt.add(row); });
        for (String input : List.of("{\"x\":1.0,\"y\":1e3}","\"{x:1b,y:[I;2,3]}\"","\"{id:'minecraft:pig',x:1b}\"","\"[]\"","\"{x:}\"","[]","{\"id\":\"minecraft:pig\"}")) {
            var parsed=JsonParser.parseString(input); var row=new JsonObject(); row.addProperty("input",input);
            row.addProperty("compound",compound(JsonOps.INSTANCE,parsed)); row.addProperty("typed",typed(JsonOps.INSTANCE,parsed));
            // The alternative must work with StringTag as well as JSON strings.
            row.addProperty("nbt_compound",compound(NbtOps.INSTANCE,JsonOps.INSTANCE.convertTo(NbtOps.INSTANCE,parsed)));
            compounds.add(row);
        }
        var nested = new JsonArray();
        var detailsClass = Class.forName("net.minecraft.world.effect.MobEffectInstance$Details");
        var detailsField = detailsClass.getDeclaredField("MAP_CODEC"); detailsField.setAccessible(true);
        var nestedCodecs = new LinkedHashMap<String,Codec<?>>();
        nestedCodecs.put("fireworks",Fireworks.CODEC);
        nestedCodecs.put("details",((MapCodec<?>)detailsField.get(null)).codec());
        nestedCodecs.put("teleport",net.minecraft.world.item.consume_effects.TeleportRandomlyConsumeEffect.CODEC.codec());
        for (var entry : nestedCodecs.entrySet()) {
            String nbtText = switch(entry.getKey()) {
                case "fireworks" -> "{flight_duration:257L,explosions:[{shape:'small_ball',has_trail:0.5d,has_twinkle:0b}]}";
                case "details" -> "{amplifier:257L,duration:4294967297L,ambient:0.5d,show_particles:0b,hidden_effect:{amplifier:2b,ambient:1b}}";
                default -> "{diameter:2.9d,directional_particles:0b}";
            };
            var row=new JsonObject(); row.addProperty("codec",entry.getKey());row.addProperty("nbt",nbtText);
            row.addProperty("expected",normalized(entry.getValue(),NbtOps.INSTANCE,TagParser.parseCompoundFully(nbtText)));
            nested.add(row);
        }
        root.add("nested",nested);
        root.add("json",json);root.add("nbt",nbt);root.add("compounds",compounds);
        Files.writeString(Path.of(args[0]),JSON.toJson(root)+"\n");
        System.out.println("Primitive vectors: "+json.size()+" JSON, "+nbt.size()+" NBT, "+compounds.size()+" compound alternatives");
    }
}
