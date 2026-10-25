import com.google.gson.*;
import com.mojang.serialization.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.particles.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.server.Bootstrap;

/** Executes ParticleTypes.CODEC against the real NbtOps and JsonOps. */
public final class ParticleCodecVectors {
    private static final Gson JSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    private static RegistryAccess registries;
    private static <T> void capture(JsonArray rows, String format, String text, DynamicOps<T> ops, T input) {
        var row = new JsonObject();
        row.addProperty("format", format); row.addProperty("input", text);
        var result = ParticleTypes.CODEC.parse(registries.createSerializationContext(ops), input);
        if (result.error().isPresent()) {
            row.addProperty("error", result.error().orElseThrow().message());
        } else {
            var particle = result.getOrThrow();
            var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(), registries);
            ParticleTypes.STREAM_CODEC.encode(buffer, particle);
            byte[] bytes = new byte[buffer.readableBytes()]; buffer.readBytes(bytes); buffer.release();
            row.addProperty("wire", HexFormat.of().formatHex(bytes));
        }
        rows.add(row);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        registries = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var rows = new JsonArray(); var inputs = new LinkedHashSet<String>();
        for (var key : new TreeSet<>(BuiltInRegistries.PARTICLE_TYPE.keySet())) inputs.add("{type:'" + key + "'}");
        inputs.addAll(List.of(
            "{type:dust,color:[1f,0f,0.5f],scale:1f}",
            "{type:dust,color:16711680,scale:0.01f}",
            "{type:dust,color:1,scale:4f}",
            "{type:dust,color:1,scale:0.001f}",
            "{type:dust,color:1,scale:5f}",
            "{type:dust,color:1,scale:'wrong'}",
            "{type:dust,color:'red',scale:1f}",
            "{type:dust,color:[1f,0f],scale:1f}",
            "{type:dust,color:[1f,0f,0f,1f],scale:1f}",
            "{type:dust,color:[I;1,0,0],scale:1f}",
            "{type:dust,color:[B;1b,0b,0b],scale:1f}",
            "{type:dust_color_transition,from_color:1,to_color:2,scale:2f}",
            "{type:dust_color_transition,from_color:[1f,0f,0f],to_color:[0f,0.5f,1f],scale:0.5f}",
            "{type:dust_color_transition,from_color:'bad',to_color:'bad',scale:9f}",
            "{type:geyser,water_blocks:3}", "{type:geyser,water_blocks:0}",
            "{type:geyser_plume,water_blocks:3}", "{type:geyser_plume,water_blocks:1.9d}",
            "{type:geyser_base,water_blocks:3,burst_impulse_base:1.5f}",
            "{type:geyser_poof,water_blocks:4,burst_impulse_base:-1f}",
            "{type:dragon_breath,power:2.5f}", "{type:dragon_breath,power:'bad'}",
            "{type:effect,color:[0.5f,0.25f,1f],power:2f}",
            "{type:instant_effect,color:123,power:0.5d}",
            "{type:effect,color:'bad',power:'bad'}",
            "{type:entity_effect,color:[1f,0f,0f,0.5f]}",
            "{type:tinted_leaves,color:[0.1f,0.2f,0.3f,0.4f]}",
            "{type:flash,color:123}", "{type:flash,color:[1f,0f,0f]}",
            "{type:sculk_charge,roll:0.5f}", "{type:sculk_charge,roll:'bad'}",
            "{type:shriek,delay:-2}", "{type:shriek,delay:4294967297L}",
            "{type:shriek,delay:2147483648d}",
            "{type:trail,target:[1d,2d,3d],color:1,duration:5}",
            "{type:trail,target:[L;1L,2L,3L],color:[0f,1f,0f],duration:5}",
            "{type:trail,target:[1d,2d],color:1,duration:0}",
            "{type:item,item:'minecraft:stone'}",
            "{type:item,item:{id:'minecraft:stone',count:2}}",
            "{type:item,item:{id:'minecraft:stone',components:{'minecraft:custom_name':'Named stone'}}}",
            "{type:block,block_state:'minecraft:stone'}",
            "{type:block_marker,block_state:{id:'minecraft:oak_log',properties:{axis:'x'}}}",
            "{type:falling_dust,block_state:{id:'minecraft:oak_log',properties:{axis:'z'}}}",
            "{type:dust_pillar,block_state:{id:'minecraft:oak_log',properties:{axis:'invalid'}}}",
            "{type:block_crumble,block_state:'minecraft:stone'}",
            "{type:block,block_state:'minecraft:invented'}",
            "{type:block,block_state:{Name:'minecraft:stone'}}",
            "{type:block,block_state:3}",
            "{type:vibration,destination:{type:block,pos:[1,2,3]},arrival_in_ticks:5}",
            "{type:vibration,destination:{type:'minecraft:block',pos:[I;1,2,3]},arrival_in_ticks:-5}",
            "{type:vibration,destination:{type:entity,source_entity:[I;0,0,0,1],y_offset:0f},arrival_in_ticks:5}",
            "{type:vibration,destination:{type:entity},arrival_in_ticks:5}",
            "{type:vibration,destination:{type:invented},arrival_in_ticks:5}",
            "{type:vibration,destination:{type:block,pos:[1,2]},arrival_in_ticks:5}",
            "{type:vibration,destination:3,arrival_in_ticks:5}",
            "{type:flame,ignored:1}", "{}", "{type:invented}", "{type:'Bad:flame'}",
            "{type:3}", "'flame'", "[]", "3"
        ));
        for (String text : inputs) {
            Tag nbt = TagParser.create(NbtOps.INSTANCE).parseFully(text);
            capture(rows, "nbt", text, NbtOps.INSTANCE, nbt);
            String json = NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, nbt).toString();
            capture(rows, "json", json, JsonOps.INSTANCE, JsonParser.parseString(json));
        }
        for (String text : List.of("null", "{\"type\":\"dust\",\"color\":true,\"scale\":1}",
                "{\"type\":\"dragon_breath\",\"power\":null}", "{\"type\":\"effect\",\"power\":true}",
                "{\"type\":\"shriek\",\"delay\":999999999999999999999999999999999999999}",
                "{\"type\":\"shriek\",\"delay\":1e20}")) {
            capture(rows, "json", text, JsonOps.INSTANCE, JsonParser.parseString(text));
        }
        Files.writeString(Path.of(args[0]), JSON.toJson(rows) + "\n");
        System.out.println("Captured " + rows.size() + " native particle codec cases");
    }
}
