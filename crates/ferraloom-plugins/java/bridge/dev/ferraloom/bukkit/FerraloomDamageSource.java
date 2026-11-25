package dev.ferraloom.bukkit;

import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.damage.DamageSource;
import org.bukkit.damage.DamageType;
import org.bukkit.entity.Entity;

/** A damage's source, as CraftDamageSource: its type, the entity that caused it and the one that dealt it. */
record FerraloomDamageSource(DamageType type, Entity causing, Entity direct, Location at) implements DamageSource {
    /** The source of damage type {@code key} ({@code minecraft:mob_attack}). */
    static FerraloomDamageSource of(String key, Entity causing, Location at) {
        DamageType type = Registry.DAMAGE_TYPE.get(NamespacedKey.fromString(key));
        return new FerraloomDamageSource(type == null ? DamageType.GENERIC : type, causing, causing, at);
    }

    @Override
    public DamageType getDamageType() {
        return type;
    }

    @Override
    public Entity getCausingEntity() {
        return causing;
    }

    @Override
    public Entity getDirectEntity() {
        return direct;
    }

    @Override
    public Location getDamageLocation() {
        return at == null ? null : at.clone();
    }

    @Override
    public Location getSourceLocation() {
        if (at != null) {
            return at.clone();
        }
        return direct == null ? null : direct.getLocation();
    }

    @Override
    public boolean isIndirect() {
        return causing != direct;
    }

    @Override
    public float getFoodExhaustion() {
        return type.getExhaustion();
    }

    /** DamageSource.scalesWithDifficulty as CraftDamageSource answers it. */
    @Override
    public boolean scalesWithDifficulty() {
        return switch (type.getDamageScaling()) {
            case NEVER -> false;
            case ALWAYS -> true;
            case WHEN_CAUSED_BY_LIVING_NON_PLAYER -> causing instanceof org.bukkit.entity.LivingEntity
                    && !(causing instanceof org.bukkit.entity.Player);
        };
    }
}
