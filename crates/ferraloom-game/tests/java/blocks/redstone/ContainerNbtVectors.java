import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;

/** Capture real saveAdditional through saveCustomOnly, including the empty
 * ContainerHelper list and the native loot-table branch. */
public final class ContainerNbtVectors {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var world = NeighborVectors.load();
        var registries = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(), world.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var lines = new ArrayList<String>();
        for (String kind : List.of("dispenser", "dropper", "hopper")) {
            for (int count : new int[]{0, 1, 64}) {
                var entity = create(kind);
                entity.setItem(0, new ItemStack(Items.STONE, count));
                lines.add(kind + ":" + count + ":false\t" + entity.saveCustomOnly(registries));
            }
            var entity = create(kind);
            entity.setLootTable(ResourceKey.create(Registries.LOOT_TABLE, Identifier.withDefaultNamespace("chests/simple_dungeon")));
            entity.setLootTableSeed(42L);
            lines.add(kind + ":0:true\t" + entity.saveCustomOnly(registries));
        }
        Files.write(Path.of(args[0]), lines);
    }
    private static RandomizableContainerBlockEntity create(String kind) {
        return switch (kind) {
            case "dispenser" -> new DispenserBlockEntity(BlockPos.ZERO, Blocks.DISPENSER.defaultBlockState());
            case "dropper" -> new DropperBlockEntity(BlockPos.ZERO, Blocks.DROPPER.defaultBlockState());
            case "hopper" -> new HopperBlockEntity(BlockPos.ZERO, Blocks.HOPPER.defaultBlockState());
            default -> throw new IllegalArgumentException(kind);
        };
    }
}
