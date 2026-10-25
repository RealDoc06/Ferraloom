import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.*;

/** Native loaded transformer providers, callbacks and updates on a real Level. */
public final class TransformerVectors {
    static final class Context extends UseOnContext {
        Context(BlockItemVectors.Fixture level, ItemStack stack, Direction face) {
            super(level, null, InteractionHand.OFF_HAND, stack, new BlockHitResult(Vec3.atCenterOf(BlockItemVectors.POS),face,BlockItemVectors.POS,false));
        }
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var pos = BlockItemVectors.POS;
        for (var item : List.of(Items.WOODEN_SHOVEL,Items.WOODEN_AXE,Items.WOODEN_HOE)) {
            for (var block : List.of(Blocks.GRASS_BLOCK,Blocks.DIRT,Blocks.COARSE_DIRT,Blocks.OAK_LOG,BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("oxidized_copper")),BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("waxed_copper_block")))) for (var state : block.getStateDefinition().getPossibleStates()) {
                for (var face : Direction.values()) for (boolean covered : new boolean[]{false,true}) {
                    var level = new BlockItemVectors.Fixture(registries);
                    level.put(pos.below(),Blocks.STONE.defaultBlockState()); level.put(pos,state);
                    if (covered) level.put(pos.above(),Blocks.STONE.defaultBlockState());
                    var stack = new ItemStack(item,3);
                    level.actions.clear(); level.getRandom().setSeed(1234);
                    var result = stack.useOn(new Context(level,stack,face));
                    System.out.println("transform\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + Block.getId(state) + "\t" + face.ordinal() + "\t" + covered
                        + "\t" + (result instanceof InteractionResult.Success ? "success" : result == InteractionResult.PASS ? "pass" : "other")
                        + "\t" + Block.getId(level.getBlockState(pos)) + "\t" + stack.getCount() + "\t" + String.join(";",level.actions) + "\t" + level.getRandom().nextLong());
                }
            }
        }
    }
}
