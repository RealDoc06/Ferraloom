import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.Heightmap;

/** Actual ChunkAccess lazy priming and LevelChunk's four-map update set. */
public final class LiveHeightmapVectors {
    public static void main(String[] args) {
        var output = System.out;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(output);
        var level = new MutationFixture(NeighborVectors.load());
        var p = new BlockPos(8, 4, 8);
        var chunk = level.getChunkAt(p);
        var section = chunk.getSection(chunk.getSectionIndex(p.getY()));
        section.setBlockState(8, 4, 8, Blocks.STONE.defaultBlockState());
        System.out.println("minimum\t" + level.getMinY());
        System.out.println("first-ocean-floor-wg\t" + level.getHeight(Heightmap.Types.OCEAN_FLOOR_WG, 8, 8));
        level.setBlock(p, Blocks.AIR.defaultBlockState(), 816, 512);
        System.out.println("cached-ocean-floor-wg\t" + level.getHeight(Heightmap.Types.OCEAN_FLOOR_WG, 8, 8));
        System.out.println("first-world-surface-wg\t" + level.getHeight(Heightmap.Types.WORLD_SURFACE_WG, 8, 8));
        level.setBlock(p.below(2), Blocks.DIRT.defaultBlockState(), 816, 512);
        System.out.println("updated-ocean-floor\t" + level.getHeight(Heightmap.Types.OCEAN_FLOOR, 8, 8));
        System.out.println("still-cached-world-surface-wg\t" + level.getHeight(Heightmap.Types.WORLD_SURFACE_WG, 8, 8));
    }
}
