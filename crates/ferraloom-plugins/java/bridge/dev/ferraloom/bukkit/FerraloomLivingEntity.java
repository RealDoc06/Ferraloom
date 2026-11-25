package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubLivingEntity;
import org.bukkit.entity.Player;

/** LivingEntity's own methods (Entity's are {@link FerraloomEntity}'s), for every living entity but players. */
final class FerraloomLivingEntity extends StubLivingEntity {
    final FerraloomEntity core;

    FerraloomLivingEntity(FerraloomEntity core) {
        this.core = core;
    }

    private double read(int field) {
        double[] s = core.state();
        return s == null ? 0 : s[field];
    }

    @Override
    public double getHealth() {
        return read(EntityNative.HEALTH);
    }

    /** CraftLivingEntity.setHealth: 0 to the maximum, and 0 kills. */
    @Override
    public void setHealth(double health) {
        double max = getMaxHealth();
        if (health < 0 || health > max) {
            throw new IllegalArgumentException("Health value (" + health + ") must be between 0 and " + max);
        }
        core.change(EntityNative.SET_HEALTH, health);
    }

    @Override
    public double getMaxHealth() {
        return read(EntityNative.MAX_HEALTH);
    }

    @Override
    public int getNoDamageTicks() {
        return (int) read(EntityNative.NO_DAMAGE);
    }

    @Override
    public Player getKiller() {
        return EntityNative.related(core.id, core.generation, EntityNative.KILLER) instanceof Player p ? p : null;
    }

    @Override
    public boolean hasAI() {
        return core.flag(EntityNative.AI);
    }

    @Override
    public void setAI(boolean ai) {
        core.change(EntityNative.SET_AI, ai ? 1 : 0);
    }

    /** CraftLivingEntity: a mob is removed when far away unless it is persistent. */
    @Override
    public boolean getRemoveWhenFarAway() {
        return !core.flag(EntityNative.PERSISTENT);
    }

    @Override
    public void setRemoveWhenFarAway(boolean remove) {
        core.change(EntityNative.SET_PERSISTENT, remove ? 0 : 1);
    }

    @Override
    public boolean isInvisible() {
        return core.flag(EntityNative.INVISIBLE);
    }
}
