package dev.ferraloom.bukkit;

import com.google.common.base.Function;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.bukkit.Bukkit;
import org.bukkit.ExplosionResult;
import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.data.BlockData;
import org.bukkit.entity.Entity;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Player;
import org.bukkit.event.entity.CreatureSpawnEvent;
import org.bukkit.event.entity.EntityChangeBlockEvent;
import org.bukkit.event.entity.EntityDamageByEntityEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.EntityDamageEvent.DamageModifier;
import org.bukkit.event.entity.EntityDeathEvent;
import org.bukkit.event.entity.EntityExplodeEvent;
import org.bukkit.event.entity.EntitySpawnEvent;
import org.bukkit.event.entity.EntityTargetEvent;
import org.bukkit.event.entity.EntityTargetLivingEntityEvent;
import org.bukkit.event.player.PlayerInteractEntityEvent;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/**
 * The entities area's event entry points (crates/ferraloom-plugins/src/entities.rs), on the server thread.
 * An actor crosses as {@code (a, b, kind)}: see {@link Entities#of}.
 */
public final class EntityEvents {
    private EntityEvents() {
    }

    /** Makes an actor's object, for natives that answer with one. */
    public static Entity of(long a, long b, String kind) {
        return Entities.of(a, b, kind);
    }

    /**
     * EntityDamageEvent, or EntityDamageByEntityEvent with a damager, before the damage is dealt: the damage to
     * deal, or NaN when cancelled. The ARMOR and MAGIC modifiers take their fractions of what is left before them,
     * so setDamage scales them as CraftBukkit's functions do; unchanged, the game's own damage ({@code dealt}) stands.
     */
    public static double damage(long va, long vb, String vk, long da, long db, String dk, String cause, String type,
            double base, double armor, double magic, double dealt) {
        try {
            Entity victim = Entities.of(va, vb, vk);
            if (victim == null) {
                return dealt;
            }
            Entity damager = Entities.of(da, db, dk);
            Map<DamageModifier, Double> modifiers = new EnumMap<>(DamageModifier.class);
            Map<DamageModifier, Function<? super Double, Double>> functions = new EnumMap<>(DamageModifier.class);
            modifiers.put(DamageModifier.BASE, base);
            functions.put(DamageModifier.BASE, f -> 0.0);
            modifiers.put(DamageModifier.ARMOR, -base * armor);
            functions.put(DamageModifier.ARMOR, f -> -f * armor);
            modifiers.put(DamageModifier.MAGIC, -(base * (1 - armor)) * magic);
            functions.put(DamageModifier.MAGIC, f -> -f * magic);
            FerraloomDamageSource source = FerraloomDamageSource.of(type, damager, null);
            EntityDamageEvent.DamageCause why = EntityDamageEvent.DamageCause.valueOf(cause);
            EntityDamageEvent event = damager == null
                    ? new EntityDamageEvent(victim, why, source, modifiers, functions)
                    : new EntityDamageByEntityEvent(damager, victim, why, source, modifiers, functions);
            double before = event.getFinalDamage();
            Bridge.server.pluginManager.callEvent(event);
            if (event.isCancelled()) {
                return Double.NaN;
            }
            double after = event.getFinalDamage();
            return after == before ? dealt : Math.max(0, after);
        } catch (Throwable ex) {
            Bridge.log(ex, "an entity taking damage");
            return dealt;
        }
    }

    /** EntityDeathEvent as the entity dies: the experience it drops. Its drops are not bridged yet (inventory area). */
    public static int death(long va, long vb, String vk, long ka, long kb, String kk, String type, int experience) {
        try {
            if (!(Entities.of(va, vb, vk) instanceof LivingEntity victim)) {
                return experience;
            }
            FerraloomDamageSource source = FerraloomDamageSource.of(type, Entities.of(ka, kb, kk), null);
            List<ItemStack> drops = new ArrayList<>();
            EntityDeathEvent event = new EntityDeathEvent(victim, source, drops, experience);
            Bridge.server.pluginManager.callEvent(event);
            return Math.max(0, event.getDroppedExp());
        } catch (Throwable ex) {
            Bridge.log(ex, "an entity dying");
            return experience;
        }
    }

    /** CreatureSpawnEvent (EntitySpawnEvent for an entity that is not alive) before it is added: true when cancelled. */
    public static boolean spawn(long a, long b, String kind, String reason) {
        try {
            Entity entity = Entities.of(a, b, kind);
            if (entity == null) {
                return false;
            }
            EntitySpawnEvent event = entity instanceof LivingEntity living
                    ? new CreatureSpawnEvent(living, CreatureSpawnEvent.SpawnReason.valueOf(reason))
                    : new EntitySpawnEvent(entity);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled();
        } catch (Throwable ex) {
            Bridge.log(ex, "an entity spawning");
            return false;
        }
    }

    /**
     * EntityTargetEvent (EntityTargetLivingEntityEvent for a living target) for a mob whose target changes:
     * {0} kept, {1} cancelled, {2} no target, {3, most, least} a player, {4, id, generation} an entity.
     */
    public static long[] target(long ma, long mb, String mk, long ta, long tb, String tk, String reason) {
        try {
            Entity mob = Entities.of(ma, mb, mk);
            Entity target = Entities.of(ta, tb, tk);
            if (mob == null) {
                return new long[] {0};
            }
            EntityTargetEvent.TargetReason why = EntityTargetEvent.TargetReason.valueOf(reason);
            EntityTargetEvent event = target instanceof LivingEntity living
                    ? new EntityTargetLivingEntityEvent(mob, living, why)
                    : new EntityTargetEvent(mob, target, why);
            Bridge.server.pluginManager.callEvent(event);
            if (event.isCancelled()) {
                return new long[] {1};
            }
            Entity now = event.getTarget();
            if (now == null ? target == null : now.equals(target)) {
                return new long[] {0};
            }
            if (now == null) {
                return new long[] {2};
            }
            if (now instanceof FerraloomPlayer p) {
                UUID id = p.id;
                return new long[] {3, id.getMostSignificantBits(), id.getLeastSignificantBits()};
            }
            FerraloomEntity e = Entities.unwrap(now);
            return e == null ? new long[] {0} : new long[] {4, e.id, e.generation};
        } catch (Throwable ex) {
            Bridge.log(ex, "a mob choosing a target");
            return new long[] {0};
        }
    }

    /**
     * EntityExplodeEvent before blocks break: the blocks that still may ({x, y, z, ...}), or null when cancelled.
     * The yield is not bridged yet.
     */
    public static int[] explode(long a, long b, String kind, int world, double x, double y, double z, float yield,
            int[] blocks) {
        try {
            Entity by = Entities.of(a, b, kind);
            FerraloomWorld in = Bridge.server.world(world);
            if (by == null || in == null) {
                return blocks;
            }
            List<Block> list = new ArrayList<>(blocks.length / 3);
            for (int i = 0; i + 2 < blocks.length; i += 3) {
                list.add(new FerraloomBlock(in, blocks[i], blocks[i + 1], blocks[i + 2], null));
            }
            EntityExplodeEvent event = new EntityExplodeEvent(by, new Location(in, x, y, z), list, yield,
                    ExplosionResult.DESTROY);
            Bridge.server.pluginManager.callEvent(event);
            if (event.isCancelled()) {
                return null;
            }
            int[] kept = new int[event.blockList().size() * 3];
            int i = 0;
            for (Block block : event.blockList()) {
                kept[i++] = block.getX();
                kept[i++] = block.getY();
                kept[i++] = block.getZ();
            }
            return kept;
        } catch (Throwable ex) {
            Bridge.log(ex, "an explosion");
            return blocks;
        }
    }

    /**
     * EntityChangeBlockEvent before an entity changes a block to state {@code to}: true when cancelled. Its
     * BlockData is the blocks area's ({@code Bukkit.createBlockData}); until that exists it is null.
     */
    public static boolean changeBlock(long a, long b, String kind, int world, int x, int y, int z, String to) {
        try {
            Entity entity = Entities.of(a, b, kind);
            FerraloomWorld in = Bridge.server.world(world);
            if (entity == null || in == null) {
                return false;
            }
            BlockData data;
            try {
                data = Bukkit.createBlockData(to);
            } catch (RuntimeException ex) {
                data = null;
            }
            EntityChangeBlockEvent event = new EntityChangeBlockEvent(entity, new FerraloomBlock(in, x, y, z, null), data);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled();
        } catch (Throwable ex) {
            Bridge.log(ex, "an entity changing a block");
            return false;
        }
    }

    /** PlayerInteractEntityEvent before the interaction: true when cancelled. */
    public static boolean interact(long most, long least, long a, long b, String kind, boolean offHand) {
        try {
            Player player = Bridge.server.online(new UUID(most, least));
            Entity entity = Entities.of(a, b, kind);
            if (player == null || entity == null) {
                return false;
            }
            PlayerInteractEntityEvent event = new PlayerInteractEntityEvent(player, entity,
                    offHand ? EquipmentSlot.OFF_HAND : EquipmentSlot.HAND);
            Bridge.server.pluginManager.callEvent(event);
            return event.isCancelled();
        } catch (Throwable ex) {
            Bridge.log(ex, "a player using an entity");
            return false;
        }
    }
}
