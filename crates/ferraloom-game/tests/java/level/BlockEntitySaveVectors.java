import java.util.List;
import java.util.stream.Stream;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.TagParser;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Actual LevelChunk#getBlockEntityNbtForSaving, including its real CHECK lookup.
 * Only enclosing Level services are mocked; chunk, factories, NBT and saves are native. */
public final class BlockEntitySaveVectors {
    static final BlockPos POS = new BlockPos(1, 1, 1);
    static RegistryAccess registries;
    static final String LIVE = "{id:'minecraft:chest',x:1,y:1,z:1,LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:17L}";

    static void sample(String kind, String pending) throws Exception {
        var level = mock(ServerLevel.class);
        when(level.registryAccess()).thenReturn(registries);
        when(level.palettedContainerFactory()).thenReturn(PalettedContainerFactory.create(registries));
        when(level.getMinY()).thenReturn(0);
        when(level.getHeight()).thenReturn(16);
        when(level.getMaxY()).thenReturn(15);
        when(level.getSectionsCount()).thenReturn(1);
        when(level.getMinSectionY()).thenReturn(0);
        when(level.getSectionIndex(anyInt())).thenAnswer(c -> ((int)c.getArgument(0)) >> 4);
        var chunk = new LevelChunk(level, new ChunkPos(0, 0));
        var state = kind.equals("empty-promotion") ? Blocks.AIR.defaultBlockState() : Blocks.CHEST.defaultBlockState();
        chunk.getSection(0).setBlockState(1, 1, 1, state);
        CompoundTag raw = pending == null ? null : TagParser.parseCompoundFully(pending);
        CompoundTag before = raw == null ? null : raw.copy();
        if (raw != null) chunk.setBlockEntityNbt(raw);
        if (!kind.equals("promoted") && !kind.equals("empty-promotion")) {
            var entity = BlockEntity.loadStatic(POS, state, TagParser.parseCompoundFully(LIVE), registries);
            if (entity == null) throw new AssertionError("live chest factory");
            chunk.setBlockEntity(entity);
            if (!kind.equals("live")) entity.setRemoved();
        }
        var saved = chunk.getBlockEntityNbtForSaving(POS, registries);
        if (raw != null && !raw.equals(before)) throw new AssertionError("mutated pending input");
        if (kind.equals("pending") && saved == raw) throw new AssertionError("pending save aliases source");
        System.out.println("VECTOR\t" + kind + "\t" + (before == null ? "-" : before) + "\t"
            + (saved == null ? "-" : saved) + "\t"
            + (chunk.getBlockEntityNbt(POS) == null ? "-" : chunk.getBlockEntityNbt(POS)) + "\t"
            + chunk.getBlockEntities().containsKey(POS));
    }

    public static void main(String[] args) throws Exception {
        var output = System.out;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(output);
        registries = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup = HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(), registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p -> p.apply());
        sample("live", "{id:'fixture:stale',x:1,y:1,z:1,keepPacked:1b}");
        sample("promoted", LIVE.substring(0, LIVE.length()-1) + ",keepPacked:1b}");
        sample("pending", "{id:'fixture:pending',x:1,y:1,z:1,keepPacked:0b,opaque:{values:[I;3,2,1]}}");
        sample("empty-promotion", "{id:'DUMMY',x:1,y:1,z:1,keepPacked:0b}");
        sample("removed", null);
    }
}
