import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.material.*;
import net.minecraft.world.level.pathfinder.PathComputationType;

/** Calls the real state virtual for each state/type; records reflected owners too. */
public final class PathfindabilityVectors {
    static Class<?> owner(Block block) throws Exception {
        for (Class<?> c=block.getClass();c!=null;c=c.getSuperclass()) {
            try {c.getDeclaredMethod("isPathfindable",BlockState.class,PathComputationType.class);return c;}
            catch (NoSuchMethodException missing) {}
        }
        throw new AssertionError(block);
    }
    static int mask(BlockState state) {
        int result=0;
        for (var type:PathComputationType.values()) if(state.isPathfindable(type)) result|=1<<type.ordinal();
        return result;
    }
    static void tags(boolean source) {
        var registry=BuiltInRegistries.FLUID;
        var water=registry.wrapAsHolder(source?Fluids.WATER:Fluids.FLOWING_WATER);
        var lava=registry.wrapAsHolder(source?Fluids.LAVA:Fluids.FLOWING_LAVA);
        registry.prepareTagReload(new TagLoader.LoadResult<>(registry.key(),Map.of(
            FluidTags.WATER,List.of(water),FluidTags.LAVA,List.of(lava)))).apply();
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        try(var manager=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack))) {
            TagLoader.loadTagsForExistingRegistries(manager,RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)).forEach(Registry.PendingTags::apply);
        }
        var classes=new HashSet<Class<?>>();var owners=new HashSet<Class<?>>();int count=0;
        try(var out=Files.newBufferedWriter(Path.of(args[0]))) {
            for(var state:Block.BLOCK_STATE_REGISTRY) {
                var block=state.getBlock();var owner=owner(block);
                if(owner!=BlockBehaviour.class) {classes.add(block.getClass());owners.add(owner);}
                out.write(Block.getId(state)+"\t"+mask(state)+"\t"+block.getClass().getName()+"\t"+owner.getName()+"\n");
                count++;
            }
        }
        if(classes.size()!=118 || owners.size()!=60) throw new AssertionError(classes.size()+" classes / "+owners.size()+" owners");
        int custom=0;
        try(var out=Files.newBufferedWriter(Path.of(args[1]))) {
            for(boolean source:new boolean[]{false,true}) {
                tags(source);
                for(var block:List.of(Blocks.WATER,Blocks.LAVA)) for(var state:block.getStateDefinition().getPossibleStates()) {
                    out.write((source?"source":"flowing")+"\t"+Block.getId(state)+"\t"+mask(state)+"\n");custom++;
                }
            }
        }
        System.out.println("PASS "+count+" native states x3 types; 118 override classes /60 native bodies; "+custom+" reloaded-tag states x3 types");
    }
}
