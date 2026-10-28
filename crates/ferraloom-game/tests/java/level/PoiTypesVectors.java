import java.nio.file.*;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.Block;

/** Exact real vanilla bootstrap: ids, names, capacities, ranges and every state. */
public final class PoiTypesVectors {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var rows = new StringBuilder();
        var registry = BuiltInRegistries.POINT_OF_INTEREST_TYPE;
        for (var type : registry) {
            rows.append(registry.getId(type)).append('\t').append(registry.getKey(type)).append('\t')
                .append(type.maxTickets()).append('\t').append(type.validRange()).append('\t')
                .append(type.matchingStates().stream().mapToInt(Block::getId).sorted().mapToObj(Integer::toString).collect(Collectors.joining(",")))
                .append('\n');
        }
        Files.writeString(Path.of(args[0]), rows.toString());
    }
}
