import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.*;

/** Runs real block placement virtuals on a Level with explicit cell/border inputs. */
public final class ContainerPlacementVectors {
    static final BlockPos POS = new BlockPos(8, 5, 8);
    static final Direction[] SIDES = {Direction.NORTH, Direction.EAST, Direction.SOUTH, Direction.WEST};
    static final class PlacementLevel extends NeighborFixture {
        final WorldBorder border = new WorldBorder();
        PlacementLevel(RegistryAccess registries) { super(registries, 100); }
        @Override public BlockState getBlockState(BlockPos pos) { return blocks.getOrDefault(pos, Blocks.AIR.defaultBlockState()); }
        @Override public FluidState getFluidState(BlockPos pos) { return getBlockState(pos).getFluidState(); }
        @Override public WorldBorder getWorldBorder() { return border; }
        void seed(long seed) { random.setSeed(seed); }
        int nextRandom() { return random.nextInt(); }
    }
    static final class PlacementPlayer extends Player {
        PlacementPlayer(Level level) { super(level, new com.mojang.authlib.GameProfile(new UUID(1, 2), "placement")); }
        @Override public GameType gameMode() { return GameType.SURVIVAL; }
    }
    static final class Context extends BlockPlaceContext {
        Context(Level level, Player player, ItemStack item, Direction face) {
            super(level, player, InteractionHand.MAIN_HAND, item, new BlockHitResult(Vec3.atCenterOf(POS), face, POS, false));
        }
    }
    static BlockState[] FLUIDS;
    static void capture(String group, Block block, float yaw, float pitch, boolean secondary, Direction face,
                        int directional, BlockState replaced, int borderMode, BlockState[] neighbors,
                        PlacementLevel level, PlacementPlayer player) {
        capture(group, block, yaw, pitch, secondary, face, directional, replaced, borderMode, neighbors, level, player, yaw);
    }
    static void capture(String group, Block block, float yaw, float pitch, boolean secondary, Direction face,
                        int directional, BlockState replaced, int borderMode, BlockState[] neighbors,
                        PlacementLevel level, PlacementPlayer player, float headYaw) {
        level.blocks.clear();
        level.blocks.put(POS, replaced);
        for (int i = 0; i < 4; i++) level.blocks.put(POS.relative(SIDES[i]), neighbors[i]);
        player.setYRot(yaw); player.setXRot(pitch); player.setYHeadRot(headYaw); player.setShiftKeyDown(secondary);
        Direction forward = directional < 0 ? player.getDirection() : Direction.from3DDataValue(directional);
        if (forward.getAxis().isVertical()) forward = Direction.NORTH;
        BlockPos head = POS.relative(forward);
        switch (borderMode) {
            case 0 -> { level.border.setCenter(0, 0); level.border.setSize(59999968); }
            case 1 -> { level.border.setCenter(POS.getX() + 0.5, POS.getZ() + 0.5); level.border.setSize(1); }
            case 2 -> { level.border.setCenter(head.getX() + 0.5, head.getZ() + 0.5); level.border.setSize(1); }
            case 3 -> { level.border.setCenter(head.getX() + 0.5, head.getZ() + 0.5); level.border.setSize(2); }
            case 4 -> { level.border.setCenter(head.getX() - 0.5, head.getZ() - 0.5); level.border.setSize(1); }
            default -> throw new AssertionError(borderMode);
        }
        var held = new ItemStack(block.asItem());
        BlockPlaceContext context = directional < 0 ? new Context(level, player, held, face)
            : new DirectionalPlaceContext(level, POS, Direction.from3DDataValue(directional), held, face);
        level.seed(263777);
        BlockState state = block.getStateForPlacement(context);
        String cells = String.join(",", Arrays.stream(neighbors).map(s -> Integer.toString(Block.getId(s))).toList());
        System.out.println("placement\t" + group + "\t" + BuiltInRegistries.BLOCK.getId(block)
            + "\t" + yaw + "\t" + pitch + "\t" + secondary + "\t" + face.get3DDataValue()
            + "\t" + directional + "\t" + Block.getId(replaced) + "\t" + borderMode + "\t" + cells
            + "\t" + (state == null ? -1 : Block.getId(state)) + "\t" + level.nextRandom()
            + "\t" + Integer.toUnsignedString(Float.floatToRawIntBits(player.getYHeadRot()),16));
    }
    static BlockState[] empty() {
        BlockState[] cells = new BlockState[4]; Arrays.fill(cells, Blocks.AIR.defaultBlockState()); return cells;
    }
    static boolean supported(Block block) {
        return block instanceof AbstractBedBlock || block instanceof AbstractFurnaceBlock || block instanceof ChestBlock
            || block instanceof BarrelBlock || block instanceof EnderChestBlock || block instanceof ShulkerBoxBlock;
    }
    public static void main(String[] args) {
        var out = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        FLUIDS = new BlockState[]{
        Blocks.AIR.defaultBlockState(), Blocks.WATER.defaultBlockState(),
        Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL, 3), Blocks.LAVA.defaultBlockState()
    };
        var registries = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var components = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(), registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(components).forEach(DataComponentInitializers.PendingComponents::apply);
        var level = new PlacementLevel(registries); var player = new PlacementPlayer(level);
        var blocks = BuiltInRegistries.BLOCK.stream().filter(ContainerPlacementVectors::supported).toList();
        var chests = blocks.stream().filter(b -> b instanceof ChestBlock).toList();
        for (var block : blocks) {
            for (float yaw : new float[]{0, 90, 180, 270}) for (float pitch : new float[]{-90, -45, 0, 45, 90})
                for (var face : Direction.values()) for (var replaced : FLUIDS)
                    capture("orient", block, yaw, pitch, false, face, -1, replaced, 0, empty(), level, player);
            for (var forward : Direction.values()) for (var face : Direction.values())
                capture("directional", block, 0, 0, false, face, forward.get3DDataValue(), FLUIDS[0], 0, empty(), level, player);
        }
        // Solid clicks redirect the placement cell; waterlogging must read the
        // adjacent target, never the clicked solid cell.
        for (var block : blocks) for (var face : Direction.values()) {
            var targets = face.getAxis().isHorizontal() ? FLUIDS : new BlockState[]{FLUIDS[0]};
            for (var target : targets) {
                var cells = empty();
                int index = Arrays.asList(SIDES).indexOf(face);
                if (index >= 0) cells[index] = target;
                capture("redirect", block, 0, 0, false, face, -1, Blocks.STONE.defaultBlockState(), 0, cells, level, player);
            }
        }
        // All four occupancy bits: clockwise takes priority when both candidates exist.
        for (var block : chests) for (int mask = 0; mask < 16; mask++) for (float yaw : new float[]{0, 90, 180, 270})
            for (boolean secondary : new boolean[]{false, true}) for (var face : Direction.values()) for (var replaced : FLUIDS) {
                var cells = empty(); var facing = Direction.fromYRot(yaw).getOpposite();
                for (int i = 0; i < 4; i++) if ((mask & (1 << i)) != 0)
                    cells[i] = block.defaultBlockState().setValue(ChestBlock.FACING, facing);
                capture("chest-mask", block, yaw, 0, secondary, face, -1, replaced, 0, cells, level, player);
            }
        // Every partner class, facing, already-joined type and face override.
        for (var block : chests) for (var partner : chests) for (var facing : SIDES) for (var type : ChestType.values())
            for (boolean secondary : new boolean[]{false, true}) for (var face : Direction.values()) {
                var cells = empty(); cells[3] = partner.defaultBlockState().setValue(ChestBlock.FACING, facing).setValue(ChestBlock.TYPE, type);
                capture("chest-partner", block, 180, 0, secondary, face, -1, FLUIDS[1], 0, cells, level, player);
            }
        var heads = List.of(Blocks.AIR.defaultBlockState(), Blocks.STONE.defaultBlockState(), FLUIDS[1], FLUIDS[2], FLUIDS[3],
            Blocks.SHORT_GRASS.defaultBlockState(), Blocks.SNOW.defaultBlockState(),
            Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, 2), Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, 8), BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("white_bed")).defaultBlockState());
        for (var block : blocks) if (block instanceof AbstractBedBlock)
            for (float yaw : new float[]{0, 90, 180, 270}) for (var head : heads) for (int border = 0; border < 5; border++) {
                var cells = empty(); cells[Arrays.asList(SIDES).indexOf(Direction.fromYRot(yaw))] = head;
                capture("bed-head", block, yaw, 0, false, Direction.UP, -1, FLUIDS[0], border, cells, level, player);
            }
        // Nearest-looking diagonal/axis ties, both ordinary and pitched barrel placement.
        for (float yaw : new float[]{45, 135, 225, 315, -45}) for (float pitch : new float[]{-45, -35.26439f, 0, 35.26439f, 45})
            capture("barrel-tie", Blocks.BARREL, yaw, pitch, false, Direction.UP, -1, FLUIDS[0], 0, empty(), level, player);
        // Direction.orderedByNearest reads the independent head rotation;
        // horizontal placement still reads body yaw through Entity#getDirection.
        for (float yaw : new float[]{0,90,180,270}) for (float headYaw : new float[]{0,90,180,270})
            for (float pitch : new float[]{-45,45})
                capture("head-rotation", Blocks.BARREL, yaw, pitch, false, Direction.UP, -1, FLUIDS[0], 0, empty(), level, player, headYaw);
    }
}
