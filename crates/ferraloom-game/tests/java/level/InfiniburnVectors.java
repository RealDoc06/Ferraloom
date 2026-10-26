import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.util.List;
import java.util.stream.Collectors;
import java.util.stream.Stream;
import net.minecraft.SharedConstants;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.dimension.DimensionType;

/** Real DimensionType#DIRECT_CODEC and BlockState#is(HolderSet), with loaded vanilla tags. */
public final class InfiniburnVectors {
    public static void main(String[] args) {
        var output = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
        var registries = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup = HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(), registries.listRegistries()));
        var ops = lookup.createSerializationContext(JsonOps.INSTANCE);
        var dimensions = registries.lookupOrThrow(Registries.DIMENSION_TYPE);
        DimensionType overworld = null;
        for (var name : List.of("overworld", "the_nether", "the_end")) {
            var type = dimensions.getOrThrow(ResourceKey.create(Registries.DIMENSION_TYPE, Identifier.withDefaultNamespace(name))).value();
            if (name.equals("overworld")) overworld = type;
            var encoded = DimensionType.DIRECT_CODEC.encodeStart(ops, type).getOrThrow().getAsJsonObject();
            emit(name, encoded.get("infiniburn").toString(), type);
        }
        var base = DimensionType.DIRECT_CODEC.encodeStart(ops, overworld).getOrThrow().getAsJsonObject();
        for (var spec : List.of("\"minecraft:stone\"", "[]", "[\"minecraft:stone\",\"minecraft:bedrock\"]")) {
            var encoded = base.deepCopy();
            encoded.add("infiniburn", JsonParser.parseString(spec));
            emit("direct", spec, DimensionType.DIRECT_CODEC.parse(ops, encoded).getOrThrow());
        }
    }

    private static void emit(String name, String spec, DimensionType type) {
        var members = BuiltInRegistries.BLOCK.stream()
            .filter(block -> block.defaultBlockState().is(type.infiniburn()))
            .map(block -> BuiltInRegistries.BLOCK.getKey(block).toString())
            .sorted().collect(Collectors.joining(","));
        System.out.println("infiniburn\t" + name + "\t" + spec + "\t" + members);
    }
}
