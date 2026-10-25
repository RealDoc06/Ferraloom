import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.ChestType;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.*;

/** Real ChestBlock/CopperChestBlock shape virtuals and Block's six-side alias. */
public final class ChestShapeVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final Direction[] SIDES = {Direction.NORTH, Direction.EAST, Direction.SOUTH, Direction.WEST};
    static final class World extends NeighborFixture {
        final LevelChunkTicks<Fluid> fluidTicks = new LevelChunkTicks<>();
        long ordinal;
        World(RegistryAccess access) { super(access, 100); }
        @Override public BlockState getBlockState(BlockPos pos) { return blocks.getOrDefault(pos, Blocks.AIR.defaultBlockState()); }
        @Override public long getGameTime() { return 0; }
        @Override public long nextSubTickCount() { return ordinal++; }
        @Override public void scheduleTick(BlockPos pos, Fluid fluid, int delay) { fluidTicks.schedule(createTick(pos, fluid, delay)); }
        void reset() { blocks.clear(); fluidTicks.removeIf(t -> true); ordinal = 0; random.setSeed(263777); }
        int nextRandom() { return random.nextInt(); }
    }
    static void capture(World world, BlockState state, Direction side, List<BlockState> neighbors) {
        world.reset();
        BlockState result;
        if (side != null) {
            result = state.updateShape(world, world, POS, side, POS.relative(side), neighbors.getFirst(), world.getRandom());
        } else {
            for (int i = 0; i < 6; i++) world.blocks.put(POS.relative(Direction.values()[i]), neighbors.get(i));
            result = Block.updateFromNeighbourShapes(state, world, POS);
        }
        var ticks = world.fluidTicks.getAll().toList();
        if (ticks.size() > 1 || ticks.stream().anyMatch(t -> t.type() != net.minecraft.world.level.material.Fluids.WATER || !t.pos().equals(POS) || t.priority() != TickPriority.NORMAL)) throw new AssertionError(ticks);
        System.out.println("chest-shape\t" + Block.getId(state) + "\t" + (side == null ? -1 : side.get3DDataValue())
            + "\t" + String.join(",", neighbors.stream().map(s -> Integer.toString(Block.getId(s))).toList())
            + "\t" + Block.getId(result) + "\t" + (ticks.isEmpty() ? -1 : ticks.getFirst().triggerTick()) + "\t" + world.nextRandom());
    }
    public static void main(String[] args) {
        var output = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
        var world = new World(NeighborVectors.load());
        var chests = BuiltInRegistries.BLOCK.stream().filter(b -> b instanceof ChestBlock).toList();
        for (var chest : chests) for (var state : chest.getStateDefinition().getPossibleStates()) {
            var neighbors = new ArrayList<BlockState>(List.of(Blocks.AIR.defaultBlockState(), Blocks.STONE.defaultBlockState(),
                Blocks.OAK_SLAB.defaultBlockState(), Blocks.BARREL.defaultBlockState(), Blocks.ENDER_CHEST.defaultBlockState()));
            for (var facing : SIDES) for (var type : ChestType.values())
                neighbors.add(chest.defaultBlockState().setValue(ChestBlock.FACING, facing).setValue(ChestBlock.TYPE, type));
            for (var neighbor : neighbors) for (var side : Direction.values()) capture(world, state, side, List.of(neighbor));
            // Every trapped/ordinary/copper/wax/oxidation pair. The neighbor's
            // WATERLOGGED differs so withPropertiesOf must preserve our value.
            for (var other : chests) if (other != chest) for (var type : ChestType.values()) for (var side : Direction.values()) {
                var neighbor = other.defaultBlockState().setValue(ChestBlock.FACING, state.getValue(ChestBlock.FACING))
                    .setValue(ChestBlock.TYPE, type).setValue(ChestBlock.WATERLOGGED, !state.getValue(ChestBlock.WATERLOGGED));
                capture(world, state, side, List.of(neighbor));
            }
            // The alias re-dispatches each intermediate state in native order.
            // Mixed copper neighbors make that order observable in the variant.
            for (int mixed = 0; mixed < 2; mixed++) for (int mask = 0; mask < 16; mask++) for (var type : ChestType.values()) {
                var cells = new ArrayList<BlockState>(Collections.nCopies(6, Blocks.AIR.defaultBlockState()));
                for (int i = 0; i < 4; i++) if ((mask & (1 << i)) != 0) {
                    var other = mixed == 0 ? chest : chests.get((chests.indexOf(chest) + i + 1) % chests.size());
                    cells.set(SIDES[i].get3DDataValue(), other.defaultBlockState().setValue(ChestBlock.FACING, state.getValue(ChestBlock.FACING)).setValue(ChestBlock.TYPE, type));
                }
                capture(world, state, null, cells);
            }
        }
    }
}
