package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubMob;
import org.bukkit.entity.AnimalTamer;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Player;

/**
 * Mob's methods, and the typed interfaces' that every mob type answers
 * alike: Tameable, Ageable. A proxy routes a type's call here only when its
 * interface has the method ({@link Entities}).
 */
final class FerraloomMob extends StubMob {
    final FerraloomEntity core;
    final FerraloomLivingEntity living;

    FerraloomMob(FerraloomEntity core, FerraloomLivingEntity living) {
        this.core = core;
        this.living = living;
    }

    @Override
    public LivingEntity getTarget() {
        return EntityNative.related(core.id, core.generation, EntityNative.TARGET) instanceof LivingEntity l ? l : null;
    }

    /** Mob.setTarget as CraftMob calls it (TargetReason.CUSTOM fires no event on Ferraloom yet). */
    @Override
    public void setTarget(LivingEntity target) {
        if (target == null) {
            EntityNative.setTarget(core.id, core.generation, 0, 0, null);
        } else if (target instanceof FerraloomPlayer p) {
            EntityNative.setTarget(core.id, core.generation, p.id.getMostSignificantBits(), p.id.getLeastSignificantBits(),
                    "player");
        } else {
            FerraloomEntity e = Entities.unwrap(target);
            if (e != null) {
                EntityNative.setTarget(core.id, core.generation, e.id, e.generation, e.type.getKeyOrThrow().getKey());
            }
        }
    }

    // Tameable.

    public boolean isTamed() {
        return core.flag(EntityNative.TAMED);
    }

    public void setTamed(boolean tamed) {
        core.change(EntityNative.SET_TAMED, tamed ? 1 : 0);
    }

    /** The owner while they are online; null otherwise (offline players are the server area's). */
    public AnimalTamer getOwner() {
        return EntityNative.related(core.id, core.generation, EntityNative.OWNER) instanceof Player p ? p : null;
    }

    // Ageable.

    public boolean isAdult() {
        return !core.flag(EntityNative.BABY);
    }

    public void setBaby() {
        core.change(EntityNative.SET_BABY, 1);
    }

    public void setAdult() {
        core.change(EntityNative.SET_BABY, 0);
    }
}
