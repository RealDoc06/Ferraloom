package dev.ferraloom.bukkit;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import org.bukkit.ExplosionResult;
import org.bukkit.Material;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.block.sign.Side;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.block.BlockBurnEvent;
import org.bukkit.event.block.BlockExplodeEvent;
import org.bukkit.event.block.BlockFadeEvent;
import org.bukkit.event.block.BlockFormEvent;
import org.bukkit.event.block.BlockFromToEvent;
import org.bukkit.event.block.BlockGrowEvent;
import org.bukkit.event.block.BlockIgniteEvent;
import org.bukkit.event.block.BlockPhysicsEvent;
import org.bukkit.event.block.BlockPistonExtendEvent;
import org.bukkit.event.block.BlockPistonRetractEvent;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.block.BlockRedstoneEvent;
import org.bukkit.event.block.BlockSpreadEvent;
import org.bukkit.event.block.LeavesDecayEvent;
import org.bukkit.event.block.SignChangeEvent;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/**
 * The blocks area's event entry points (crates/ferraloom-plugins/src/blocks.rs, same names and signatures),
 * on the server thread. Each is called only while its event has a listener, and catches what escapes it.
 */
public final class BlockEvents {
    // simple()'s kinds: src/blocks.rs, Simple.
    static final int PHYSICS = 0;
    static final int FROM_TO = 1;
    static final int BURN = 2;
    static final int IGNITE = 3;
    static final int GROW = 4;
    static final int FADE = 5;
    static final int FORM = 6;
    static final int SPREAD = 7;
    static final int LEAVES_DECAY = 8;
    static final int REDSTONE = 9;

    /** Bukkit's BlockFace ordinals as src/blocks.rs sends them (north, east, south, west, up, down). */
    private static final BlockFace[] FACES = BlockFace.values();

    private BlockEvents() {
    }

    private static FerraloomBlock block(int world, int x, int y, int z, int captured) {
        return new FerraloomBlock(Bridge.server.world(world), x, y, z, captured);
    }

    private static boolean call(Event event) {
        Bridge.server.pluginManager.callEvent(event);
        return event instanceof Cancellable c && c.isCancelled();
    }

    /**
     * ServerPlayerGameMode.destroyBlock's BlockBreakEvent, before anything breaks, with the experience the
     * block would drop: bit 32 cancelled, bit 33 no drops, the low bits the experience to drop.
     */
    public static long blockBreak(long most, long least, int world, int x, int y, int z, int state, int exp) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        if (player == null || Bridge.server.world(world) == null) {
            return exp & 0xFFFFFFFFL;
        }
        try {
            BlockBreakEvent event = new BlockBreakEvent(block(world, x, y, z, state), player);
            event.setExpToDrop(exp);
            boolean cancelled = call(event);
            return (cancelled ? 1L << 32 : 0) | (event.isDropItems() ? 0 : 1L << 33) | (event.getExpToDrop() & 0xFFFFFFFFL);
        } catch (Throwable ex) {
            Bridge.log(ex, "a block breaking");
            return exp & 0xFFFFFFFFL;
        }
    }

    /** BlockItem.place's BlockPlaceEvent, before the block is placed: true when cancelled or canBuild is false. */
    public static boolean blockPlace(long most, long least, int world, int x, int y, int z, int placed, int replaced,
            int ax, int ay, int az, boolean canBuild) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        FerraloomWorld in = Bridge.server.world(world);
        if (player == null || in == null) {
            return false;
        }
        try {
            Material item = Material.matchMaterial(Blocks.kind(placed).item);
            ItemStack hand = new ItemStack(item == null ? Material.AIR : item, 1);
            BlockPlaceEvent event = new BlockPlaceEvent(block(world, x, y, z, placed),
                    new FerraloomBlockState(in, x, y, z, replaced), new FerraloomBlock(in, ax, ay, az), hand, player,
                    canBuild, EquipmentSlot.HAND);
            return call(event) || !event.canBuild();
        } catch (Throwable ex) {
            Bridge.log(ex, "a block placing");
            return false;
        }
    }

    /** The events with a few numbers (src/blocks.rs, Simple): 1 cancelled, 0 not; REDSTONE answers the current. */
    public static int simple(int kind, int world, int x, int y, int z, int a, int b, int c, int d, int e) {
        FerraloomWorld in = Bridge.server.world(world);
        if (in == null) {
            return kind == REDSTONE ? b : 0;
        }
        try {
            FerraloomBlock block = new FerraloomBlock(in, x, y, z);
            Event event = switch (kind) {
                case PHYSICS -> new BlockPhysicsEvent(block, Blocks.data(a), new FerraloomBlock(in, b, c, d));
                case FROM_TO -> new BlockFromToEvent(block, new FerraloomBlock(in, b, c, d));
                case BURN -> new BlockBurnEvent(block, a == 1 ? new FerraloomBlock(in, b, c, d) : null);
                case IGNITE -> new BlockIgniteEvent(block, BlockIgniteEvent.IgniteCause.values()[a],
                        e == 1 ? new FerraloomBlock(in, b, c, d) : null);
                case GROW -> new BlockGrowEvent(block, new FerraloomBlockState(in, x, y, z, a));
                case FADE -> new BlockFadeEvent(block, new FerraloomBlockState(in, x, y, z, a));
                case FORM -> new BlockFormEvent(block, new FerraloomBlockState(in, x, y, z, a));
                case SPREAD -> new BlockSpreadEvent(block, new FerraloomBlock(in, b, c, d),
                        new FerraloomBlockState(in, x, y, z, a));
                case LEAVES_DECAY -> new LeavesDecayEvent(block);
                case REDSTONE -> new BlockRedstoneEvent(block, a, b);
                default -> null;
            };
            if (event == null) {
                return kind == REDSTONE ? b : 0;
            }
            boolean cancelled = call(event);
            if (event instanceof BlockRedstoneEvent redstone) {
                return redstone.getNewCurrent();
            }
            return cancelled ? 1 : 0;
        } catch (Throwable ex) {
            Bridge.log(ex, "a block event");
            return kind == REDSTONE ? b : 0;
        }
    }

    /** PistonBaseBlock.moveBlocks's BlockPistonExtendEvent or BlockPistonRetractEvent: true when cancelled. */
    public static boolean piston(int world, int x, int y, int z, int direction, boolean extending, int[] moved) {
        FerraloomWorld in = Bridge.server.world(world);
        if (in == null) {
            return false;
        }
        try {
            List<Block> blocks = new ArrayList<>();
            for (int i = 0; i + 2 < moved.length; i += 3) {
                blocks.add(new FerraloomBlock(in, moved[i], moved[i + 1], moved[i + 2]));
            }
            FerraloomBlock piston = new FerraloomBlock(in, x, y, z);
            BlockFace face = FACES[direction];
            return call(extending ? new BlockPistonExtendEvent(piston, blocks, face)
                    : new BlockPistonRetractEvent(piston, blocks, face));
        } catch (Throwable ex) {
            Bridge.log(ex, "a piston moving");
            return false;
        }
    }

    /** SignBlockEntity.updateSignText's SignChangeEvent: the lines to keep, null when cancelled. */
    public static String[] signChange(long most, long least, int world, int x, int y, int z, boolean front, String[] lines) {
        FerraloomPlayer player = Bridge.server.online(new UUID(most, least));
        FerraloomWorld in = Bridge.server.world(world);
        if (player == null || in == null) {
            return lines;
        }
        try {
            SignChangeEvent event = new SignChangeEvent(new FerraloomBlock(in, x, y, z), player, lines.clone(),
                    front ? Side.FRONT : Side.BACK);
            return call(event) ? null : event.getLines();
        } catch (Throwable ex) {
            Bridge.log(ex, "a sign changing");
            return lines;
        }
    }

    /**
     * ServerExplosion.explode's BlockExplodeEvent: the yield's bits, then the blocks left to destroy (x y z
     * each); null when cancelled.
     */
    public static int[] explode(int world, int x, int y, int z, int state, float yield, int[] blocks) {
        FerraloomWorld in = Bridge.server.world(world);
        if (in == null) {
            return null;
        }
        try {
            List<Block> list = new ArrayList<>();
            for (int i = 0; i + 2 < blocks.length; i += 3) {
                list.add(new FerraloomBlock(in, blocks[i], blocks[i + 1], blocks[i + 2]));
            }
            BlockExplodeEvent event = new BlockExplodeEvent(new FerraloomBlock(in, x, y, z),
                    new FerraloomBlockState(in, x, y, z, state), list, yield, ExplosionResult.DESTROY);
            if (call(event)) {
                return null;
            }
            List<Block> kept = event.blockList();
            int[] out = new int[1 + kept.size() * 3];
            out[0] = Float.floatToRawIntBits(event.getYield());
            for (int i = 0; i < kept.size(); i++) {
                Block b = kept.get(i);
                out[1 + i * 3] = b.getX();
                out[2 + i * 3] = b.getY();
                out[3 + i * 3] = b.getZ();
            }
            return out;
        } catch (Throwable ex) {
            Bridge.log(ex, "an explosion");
            int[] out = new int[1 + blocks.length];
            out[0] = Float.floatToRawIntBits(yield);
            System.arraycopy(blocks, 0, out, 1, blocks.length);
            return out;
        }
    }
}
