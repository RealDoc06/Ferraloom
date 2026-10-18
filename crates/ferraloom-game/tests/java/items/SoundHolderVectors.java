import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.sounds.SoundEvent;

/** The real holder codec and SoundEvent#getRange, including direct custom IDs. */
public final class SoundHolderVectors {
    public static void main(String[] args) throws Exception {
        var out = System.out;
        net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap(); System.setOut(out);
        var registries = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var ops = registries.createSerializationContext(JsonOps.INSTANCE);
        for (var json : List.of("\"minecraft:block.stone.place\"", "{\"sound_id\":\"fixture:variable\"}", "{\"sound_id\":\"fixture:fixed\",\"range\":3.5}", "{\"sound_id\":\"fixture:negative\",\"range\":-3}", "{\"sound_id\":\"fixture:lenient\",\"range\":\"bad\"}", "\"fixture:missing\"")) {
            var decoded = SoundEvent.CODEC.parse(ops, JsonParser.parseString(json)).result();
            if (decoded.isEmpty()) { System.out.println("missing\t" + json); continue; }
            for (float volume : new float[]{-1, 0, 1, 1.5f, Float.NaN, Float.POSITIVE_INFINITY}) {
                var sound = decoded.get();
                System.out.println("sound\t" + json + "\t" + Float.floatToRawIntBits(volume) + "\t" + Float.floatToRawIntBits(sound.value().getRange(volume)) + "\t" + sound.kind());
            }
        }
    }
}
