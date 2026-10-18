import java.util.*;
import java.util.function.Predicate;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.sounds.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.BlockItemStateProperties;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.*;

/** Real ItemStack#useOn and BlockItem#place in a controlled, empty native Level. */
public final class BlockItemVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final class Context extends UseOnContext {
        Context(Fixture level, ItemStack stack, Direction face) {
            super(level, null, InteractionHand.MAIN_HAND, stack, new BlockHitResult(Vec3.atCenterOf(POS), face, POS, false));
        }
    }
    static final class Fixture extends MutationFixture {
        final List<String> actions = new ArrayList<>();
        Fixture(RegistryAccess registries) { super(registries); }
        // The fixture contains no entities. The native isUnobstructed loop and
        // context-dependent block collision methods still execute normally.
        @Override public List<Entity> getEntities(Entity except, AABB box, Predicate<? super Entity> predicate) {
            actions.add("entities");
            return List.of();
        }
        @Override public void playSeededSound(Entity except, double x, double y, double z, Holder sound, SoundSource source, float volume, float pitch, long seed) {
            actions.add("sound:" + ((SoundEvent) sound.value()).location() + ":" + Float.floatToRawIntBits(volume) + ":" + Float.floatToRawIntBits(pitch));
        }
        @Override public void gameEvent(Holder event, Vec3 at, GameEvent.Context context) {
            actions.add("event:" + ((net.minecraft.resources.ResourceKey<?>) event.unwrapKey().orElseThrow()).identifier() + ":" + Block.getId(context.affectedState()));
        }
        void put(BlockPos pos, BlockState state) {
            getChunkAt(pos).setBlockState(pos, state, 816);
        }
    }
    public static void main(String[] args) throws Exception {
        var out = System.out;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var world = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries = new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(), world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        for (var item : List.of(Items.STONE, Items.DIRT, Items.GLASS, Items.BROWN_MUSHROOM_BLOCK, Items.RED_MUSHROOM_BLOCK)) {
            for (var face : Direction.values()) for (int occupied = 0; occupied < 3; occupied++) for (boolean separate : new boolean[]{false, true}) for (int properties = 0; properties < 3; properties++) {
                var level = new Fixture(registries);
                for (var direction : Direction.values()) level.put(POS.relative(direction), (occupied == 2 ? Blocks.STONE : Blocks.AIR).defaultBlockState());
                level.put(POS, (occupied == 0 ? Blocks.AIR : Blocks.STONE).defaultBlockState());
                var receiver = new ItemStack(item, 3);
                var held = separate ? new ItemStack(Items.DIRT, 7) : receiver;
                if (properties != 0) held.set(DataComponents.BLOCK_STATE, new BlockItemStateProperties(Map.of("north", properties == 1 ? "false" : "invalid")));
                level.actions.clear(); level.trace.clear(); level.getRandom().setSeed(1234);
                var result = receiver.useOn(new Context(level, held, face));
                String outcome = result instanceof InteractionResult.Success success
                    ? "success:" + success.swingSource() + ":" + success.wasItemInteraction() + ":" + (success.heldItemTransformedTo() != null)
                    : result == InteractionResult.FAIL ? "fail" : result == InteractionResult.PASS ? "pass" : "other";
                System.out.println("place\t" + BuiltInRegistries.ITEM.getId(item) + "\t" + face.ordinal() + "\t" + occupied + "\t" + separate + "\t" + properties
                    + "\t" + outcome + "\t" + receiver.getCount() + "\t" + held.getCount()
                    + "\t" + Block.getId(level.getBlockState(POS)) + "\t" + Block.getId(level.getBlockState(POS.relative(face)))
                    + "\t" + String.join(";", level.actions) + "\t" + level.getRandom().nextLong());
            }
        }
    }
}
