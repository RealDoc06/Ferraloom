import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.WorldGenLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.feature.SimpleBlockFeature;
import net.minecraft.world.level.levelgen.feature.stateproviders.WeightedStateProvider;
import net.minecraft.util.random.WeightedList;

/** Authored test of the live RNG path used by TreeGrower and SimpleBlockFeature. */
public final class LiveFeatureRandomOracle {
    private static final BlockPos ORIGIN = new BlockPos(0, 64, 0);

    private static final class World implements InvocationHandler {
        final RandomSource random;
        final boolean drawInCallback;
        final Map<BlockPos, BlockState> blocks = new HashMap<>();
        final List<String> writes = new ArrayList<>();
        final List<Integer> callbackDraws = new ArrayList<>();

        World(long seed, boolean drawInCallback) {
            this.random = RandomSource.create(seed);
            this.drawInCallback = drawInCallback;
            for (int[] offset : new int[][] {{0, -1}, {1, 0}, {0, 1}, {-1, 0}}) {
                blocks.put(ORIGIN.offset(offset[0], 0, offset[1]), Blocks.STONE.defaultBlockState());
                blocks.put(ORIGIN.offset(offset[0], 1, offset[1]), Blocks.STONE.defaultBlockState());
            }
        }

        @Override public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
            switch (method.getName()) {
                case "getRandom": return random;
                case "getBlockState": {
                    BlockPos pos = (BlockPos) args[0];
                    return blocks.getOrDefault(pos, pos.getY() < 64 ? Blocks.STONE.defaultBlockState() : Blocks.AIR.defaultBlockState());
                }
                case "setBlock": {
                    BlockPos pos = ((BlockPos) args[0]).immutable();
                    BlockState state = (BlockState) args[1];
                    blocks.put(pos, state);
                    writes.add(pos.getX() + "," + pos.getY() + "," + pos.getZ() + "=" + Block.getId(state));
                    if (drawInCallback) callbackDraws.add(random.nextInt(97));
                    return true;
                }
                case "getMinY": return -64;
                case "getHeight":
                    if (args == null || args.length == 0) return 384;
                    throw new AssertionError("unexpected height query: " + method);
                case "toString": return "LiveFeatureRandomFixture";
                case "hashCode": return System.identityHashCode(proxy);
                case "equals": return proxy == args[0];
                default:
                    if (method.isDefault()) return InvocationHandler.invokeDefault(proxy, method, args);
                    throw new AssertionError("unhandled fixture query: " + method);
            }
        }
    }

    public static void main(String[] args) {
        var output = System.out;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        // The actual provider registered by VegetationFeatures#bootstrap.
        var feature = new SimpleBlockFeature(new WeightedStateProvider(
            WeightedList.<BlockState>builder()
                .add(Blocks.PALE_MOSS_CARPET.defaultBlockState(), 25)
                .add(Blocks.SHORT_GRASS.defaultBlockState(), 25)
                .add(Blocks.TALL_GRASS.defaultBlockState(), 10)));
        for (boolean callback : new boolean[] {false, true}) {
            int compared = 0;
            for (long seed = 0; seed < 4096 && compared < 4; seed++) {
                RandomSource expected = RandomSource.create(seed);
                if (expected.nextInt(60) >= 25) continue;
                List<Integer> expectedCallbacks = new ArrayList<>();
                if (callback) expectedCallbacks.add(expected.nextInt(97));
                boolean topper = false;
                for (int i = 0; i < 4; i++) topper |= expected.nextBoolean();
                if (topper) continue;
                World fixture = new World(seed, callback);
                WorldGenLevel world = (WorldGenLevel) Proxy.newProxyInstance(
                    WorldGenLevel.class.getClassLoader(), new Class<?>[] {WorldGenLevel.class}, fixture);
                // This concrete feature does not consult the chunk generator.
                boolean placed = feature.place(world, null, fixture.random, ORIGIN);
                long tail = fixture.random.nextLong();
                int state = Block.getId(fixture.blocks.get(ORIGIN));
                if (!placed || !fixture.writes.equals(List.of("0,64,0=" + state))
                    || !fixture.callbackDraws.equals(expectedCallbacks) || tail != expected.nextLong()) {
                    throw new AssertionError(seed + " " + callback + " " + fixture.writes
                        + " " + fixture.callbackDraws + " " + tail);
                }
                String callbacks = callback ? "Some(" + fixture.callbackDraws + ")" : "None";
                output.println("FEATURE_RANDOM_CASE\t" + seed + "\t" + callback + "\t" + state
                    + "\t" + callbacks + "\t" + tail);
                compared++;
            }
            if (compared != 4) throw new AssertionError("four no-topper cases per callback mode");
        }
    }
}
