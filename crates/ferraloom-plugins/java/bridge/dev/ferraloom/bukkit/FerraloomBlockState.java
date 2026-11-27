package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubBlockState;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.Locale;
import java.util.Map;
import org.bukkit.Chunk;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.BlockData;

/**
 * A {@link BlockState} (CraftBlockState): a snapshot of a block's state at a position, changed freely and
 * written back by {@link #update}. A block with a block entity gets a proxy implementing its state's own
 * interface ({@code Sign}, {@code Chest}...), so {@code instanceof} and casts behave; its block entity's
 * data (sign text, container contents) is not bridged yet and throws from those methods.
 */
class FerraloomBlockState extends StubBlockState {
    /** The world and position; a null world for one not placed ({@code BlockData.createBlockState}). */
    final FerraloomWorld world;
    final int x;
    final int y;
    final int z;
    int state;
    /** A sign's lines, front and back, read on first use; changed ones are written by {@link #update}. */
    private String[][] sign;
    private final boolean[] signChanged = new boolean[2];

    FerraloomBlockState(FerraloomWorld world, int x, int y, int z, int state) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
        this.state = state;
    }

    /** The block entities' state interfaces, by block name (CraftBlockStates' table, the common ones). */
    private static final Map<String, Class<? extends BlockState>> STATES = Map.ofEntries(
            Map.entry("chest", org.bukkit.block.Chest.class),
            Map.entry("trapped_chest", org.bukkit.block.Chest.class),
            Map.entry("barrel", org.bukkit.block.Barrel.class),
            Map.entry("ender_chest", org.bukkit.block.EnderChest.class),
            Map.entry("furnace", org.bukkit.block.Furnace.class),
            Map.entry("blast_furnace", org.bukkit.block.BlastFurnace.class),
            Map.entry("smoker", org.bukkit.block.Smoker.class),
            Map.entry("hopper", org.bukkit.block.Hopper.class),
            Map.entry("dispenser", org.bukkit.block.Dispenser.class),
            Map.entry("dropper", org.bukkit.block.Dropper.class),
            Map.entry("brewing_stand", org.bukkit.block.BrewingStand.class),
            Map.entry("spawner", org.bukkit.block.CreatureSpawner.class),
            Map.entry("beacon", org.bukkit.block.Beacon.class),
            Map.entry("jukebox", org.bukkit.block.Jukebox.class),
            Map.entry("lectern", org.bukkit.block.Lectern.class),
            Map.entry("bell", org.bukkit.block.Bell.class),
            Map.entry("beehive", org.bukkit.block.Beehive.class),
            Map.entry("bee_nest", org.bukkit.block.Beehive.class),
            Map.entry("command_block", org.bukkit.block.CommandBlock.class),
            Map.entry("chain_command_block", org.bukkit.block.CommandBlock.class),
            Map.entry("repeating_command_block", org.bukkit.block.CommandBlock.class),
            Map.entry("campfire", org.bukkit.block.Campfire.class),
            Map.entry("soul_campfire", org.bukkit.block.Campfire.class));

    /** The state of the block at a position: a proxy for a block entity's own state interface. */
    static BlockState of(FerraloomWorld world, int x, int y, int z, int state) {
        FerraloomBlockState base = new FerraloomBlockState(world, x, y, z, state);
        Blocks.Kind kind = Blocks.kind(state);
        if (!kind.is(Blocks.BLOCK_ENTITY)) {
            return base;
        }
        Class<? extends BlockState> type = stateType(kind.key.getKey());
        return (BlockState) Proxy.newProxyInstance(FerraloomBlockState.class.getClassLoader(),
                new Class<?>[] {type}, new Delegate(base, type));
    }

    private static Class<? extends BlockState> stateType(String name) {
        Class<? extends BlockState> known = STATES.get(name);
        if (known != null) {
            return known;
        }
        if (name.endsWith("hanging_sign")) {
            return org.bukkit.block.HangingSign.class;
        }
        if (name.endsWith("_sign")) {
            return org.bukkit.block.Sign.class;
        }
        if (name.endsWith("shulker_box")) {
            return org.bukkit.block.ShulkerBox.class;
        }
        if (name.endsWith("_banner")) {
            return org.bukkit.block.Banner.class;
        }
        if (name.endsWith("_bed")) {
            return org.bukkit.block.Bed.class;
        }
        if (name.endsWith("_skull") || name.endsWith("_head")) {
            return org.bukkit.block.Skull.class;
        }
        return org.bukkit.block.TileState.class;
    }

    /**
     * A block entity's state: BlockState's methods are the snapshot's, and a sign's lines are its text
     * (CraftSign: {@code getLine} and {@code setLine} are the front's); the rest is not bridged yet.
     */
    private record Delegate(FerraloomBlockState base, Class<?> type) implements InvocationHandler {
        @Override
        public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
            if (org.bukkit.block.Sign.class.isAssignableFrom(type)) {
                Object answer = base.sign(proxy, method, args);
                if (answer != NONE) {
                    return answer;
                }
            }
            if (method.getDeclaringClass().isAssignableFrom(FerraloomBlockState.class)) {
                try {
                    return method.invoke(base, args);
                } catch (InvocationTargetException e) {
                    throw e.getCause();
                }
            }
            if (method.isDefault()) {
                return InvocationHandler.invokeDefault(proxy, method, args);
            }
            throw new UnsupportedOperationException("Ferraloom does not implement " + type.getSimpleName() + "."
                    + method.getName() + "() yet");
        }
    }

    private static final Object NONE = new Object();

    /** The lines of one side (0 front, 1 back). */
    String[] lines(int side) {
        if (sign == null) {
            sign = new String[2][];
            for (int s = 0; s < 2; s++) {
                String[] read = world == null ? null : BlockNative.signLines(world.index, x, y, z, s == 0);
                sign[s] = read != null ? read : new String[] {"", "", "", ""};
            }
        }
        return sign[side];
    }

    /** Sign's and SignSide's line methods on side {@code side}; {@link #NONE} for the others. */
    private Object lineMethod(int side, Method method, Object[] args) {
        switch (method.getName()) {
            case "getLines" -> {
                return lines(side).clone();
            }
            case "getLine" -> {
                return lines(side)[(Integer) args[0]];
            }
            case "setLine" -> {
                String line = (String) args[1];
                lines(side)[(Integer) args[0]] = line == null ? "" : line;
                signChanged[side] = true;
                return null;
            }
            default -> {
                return NONE;
            }
        }
    }

    /** The Sign methods the snapshot answers: the front's lines, and getSide for a side's own. */
    private Object sign(Object proxy, Method method, Object[] args) {
        if (method.getName().equals("getSide") && args != null && args[0] instanceof org.bukkit.block.sign.Side side) {
            int s = side == org.bukkit.block.sign.Side.FRONT ? 0 : 1;
            return Proxy.newProxyInstance(FerraloomBlockState.class.getClassLoader(),
                    new Class<?>[] {org.bukkit.block.sign.SignSide.class}, (p, m, a) -> {
                        Object answer = lineMethod(s, m, a);
                        if (answer != NONE) {
                            return answer;
                        }
                        if (m.getDeclaringClass() == Object.class) {
                            return m.getName().equals("equals") ? p == a[0]
                                    : m.getName().equals("hashCode") ? System.identityHashCode(p) : "CraftSignSide";
                        }
                        throw new UnsupportedOperationException("Ferraloom does not implement SignSide." + m.getName() + "() yet");
                    });
        }
        return lineMethod(0, method, args);
    }

    private FerraloomWorld placed() {
        if (world == null) {
            throw new IllegalStateException("The BlockState is not placed in a world");
        }
        return world;
    }

    @Override
    public Block getBlock() {
        return new FerraloomBlock(placed(), x, y, z);
    }

    @Override
    public BlockData getBlockData() {
        return Blocks.data(state);
    }

    @Override
    public void setBlockData(BlockData data) {
        state = Blocks.state(data);
    }

    @Override
    public Material getType() {
        return Blocks.kind(state).material();
    }

    @Override
    public void setType(Material type) {
        Blocks.Kind kind = Blocks.kind(type);
        if (kind == null) {
            throw new IllegalArgumentException("Material must be a block: " + type);
        }
        if (kind != Blocks.kind(state)) {
            state = kind.defaultState;
        }
    }

    @Override
    public World getWorld() {
        return placed();
    }

    @Override
    public int getX() {
        return x;
    }

    @Override
    public int getY() {
        return y;
    }

    @Override
    public int getZ() {
        return z;
    }

    @Override
    public Location getLocation() {
        return new Location(placed(), x, y, z);
    }

    @Override
    public Location getLocation(Location into) {
        if (into != null) {
            into.setWorld(placed());
            into.setX(x);
            into.setY(y);
            into.setZ(z);
            into.setYaw(0);
            into.setPitch(0);
        }
        return into;
    }

    @Override
    public Chunk getChunk() {
        return placed().getChunkAt(x >> 4, z >> 4);
    }

    @Override
    public byte getLightLevel() {
        return getBlock().getLightLevel();
    }

    @Override
    public boolean isPlaced() {
        return world != null;
    }

    @Override
    public boolean update() {
        return update(false);
    }

    @Override
    public boolean update(boolean force) {
        return update(force, true);
    }

    /** CraftBlockState.update: false when the block changed type since (unless forced); else the state is written. */
    @Override
    public boolean update(boolean force, boolean applyPhysics) {
        FerraloomWorld w = placed();
        int now = BlockNative.state(w.index, x, y, z);
        if (now < 0) {
            return false;
        }
        if (Blocks.kind(now) != Blocks.kind(state) && !force) {
            return false;
        }
        if (now != state) {
            BlockNative.setState(w.index, x, y, z, state, applyPhysics);
        }
        for (int s = 0; s < 2; s++) {
            if (signChanged[s]) {
                BlockNative.setSignLines(w.index, x, y, z, s == 0, sign[s]);
                signChanged[s] = false;
            }
        }
        return true;
    }

    @Override
    public BlockState copy() {
        return new FerraloomBlockState(world, x, y, z, state);
    }

    @Override
    public BlockState copy(Location at) {
        FerraloomWorld w = at.getWorld() instanceof FerraloomWorld fw ? fw : world;
        return new FerraloomBlockState(w, at.getBlockX(), at.getBlockY(), at.getBlockZ(), state);
    }

    @Override
    public String toString() {
        return "CraftBlockState{" + Blocks.asString(state, -1L).toLowerCase(Locale.ROOT) + " at " + x + "," + y + ","
                + z + "}";
    }
}
