import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.chunk.status.*;

/** Dependency vectors captured directly from the bootstrapped 26.3 registry. */
public final class PyramidVectors {
    static String dependencies(ChunkDependencies deps) {
        return deps.asList().stream().map(ChunkStatus::getName).collect(Collectors.joining(","));
    }
    public static void main(String[] args) {
        var out = System.out;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(out);
        for (var name : new String[]{"generation", "loading"}) {
            var pyramid = name.equals("generation") ? ChunkPyramid.GENERATION_PYRAMID : ChunkPyramid.LOADING_PYRAMID;
            for (var status : ChunkStatus.getStatusList()) {
                var step = pyramid.getStepTo(status);
                System.out.println(name+"\t"+status.getName()+"\t"+dependencies(step.directDependencies())+"\t"+dependencies(step.accumulatedDependencies())+"\t"+step.blockStateWriteRadius());
            }
        }
    }
}
