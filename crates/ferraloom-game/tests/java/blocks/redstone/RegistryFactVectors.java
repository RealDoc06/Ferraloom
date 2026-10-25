import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.dispenser.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.properties.NoteBlockInstrument;

/** Independent bootstrap observations, without reading the extractor's output
 * or classifying a projectile position function with a copied formula. */
public final class RegistryFactVectors {
    private static JsonObject items() {
        var result = new JsonObject();
        var entries = new JsonArray();
        for (var item : BuiltInRegistries.ITEM) {
            var entry = new JsonObject();
            entry.addProperty("id", BuiltInRegistries.ITEM.getId(item));
            entry.addProperty("name", BuiltInRegistries.ITEM.getKey(item).toString());
            entry.addProperty("spawn_egg", item instanceof SpawnEggItem);
            entry.addProperty("registered", DispenserBlock.DISPENSER_REGISTRY.containsKey(item));
            var behavior = DispenserBlock.DISPENSER_REGISTRY.get(item);
            entry.addProperty("behavior", behavior == null ? null : behavior == DispenseItemBehavior.NOOP ? "Noop" : behavior.getClass().getName());
            JsonElement projectile = JsonNull.INSTANCE;
            if (item instanceof ProjectileItem projectileItem) {
                var config = projectileItem.createDispenseConfig();
                var values = new JsonObject();
                values.addProperty("power_bits", Integer.toUnsignedLong(Float.floatToRawIntBits(config.power())));
                values.addProperty("uncertainty_bits", Integer.toUnsignedLong(Float.floatToRawIntBits(config.uncertainty())));
                values.addProperty("event", config.overrideDispenseEvent().orElse(null));
                var positions = new JsonArray();
                for (var direction : Direction.values()) {
                    var source = new BlockSource(null, new BlockPos(8, 64, 8), Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING, direction), null);
                    var actual = config.positionFunction().getDispensePosition(source, direction);
                    var position = new JsonObject();
                    position.addProperty("direction", direction.getSerializedName());
                    var bits = new JsonArray();
                    bits.add(Long.toHexString(Double.doubleToRawLongBits(actual.x())));
                    bits.add(Long.toHexString(Double.doubleToRawLongBits(actual.y())));
                    bits.add(Long.toHexString(Double.doubleToRawLongBits(actual.z())));
                    position.add("bits", bits);
                    positions.add(position);
                }
                values.add("positions", positions);
                projectile = values;
            }
            entry.add("projectile", projectile);
            entries.add(entry);
        }
        result.add("items", entries);
        return result;
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var result = items();
        var blocks = new JsonArray();
        for (var block : BuiltInRegistries.BLOCK) {
            var instrument = block.defaultBlockState().instrument();
            // Ferraloom models this constructor fact once per block. Verify
            // that sharing against every real state before emitting the fact.
            for (var state : block.getStateDefinition().getPossibleStates()) {
                if (state.instrument() != instrument) throw new AssertionError("State-dependent instrument: " + state);
            }
            var entry = new JsonObject();
            entry.addProperty("id", BuiltInRegistries.BLOCK.getId(block));
            entry.addProperty("name", BuiltInRegistries.BLOCK.getKey(block).toString());
            entry.addProperty("state_count", block.getStateDefinition().getPossibleStates().size());
            entry.addProperty("instrument", instrument.getSerializedName());
            blocks.add(entry);
        }
        result.add("blocks", blocks);
        result.addProperty("state_count", Block.BLOCK_STATE_REGISTRY.size());
        var instruments = new JsonArray();
        for (var instrument : NoteBlockInstrument.values()) {
            var entry = new JsonObject();
            entry.addProperty("name", instrument.getSerializedName());
            var sound = instrument.getSoundEvent().value();
            entry.addProperty("sound_id", BuiltInRegistries.SOUND_EVENT.getId(sound));
            entry.addProperty("sound_name", BuiltInRegistries.SOUND_EVENT.getKey(sound).toString());
            entry.addProperty("tunable", instrument.isTunable());
            entry.addProperty("custom_sound", instrument.hasCustomSound());
            entry.addProperty("works_above", instrument.worksAboveNoteBlock());
            instruments.add(entry);
        }
        result.add("instruments", instruments);
        Files.writeString(Path.of(args[0]), new GsonBuilder().serializeNulls().setPrettyPrinting().create().toJson(result) + "\n");
    }
}
