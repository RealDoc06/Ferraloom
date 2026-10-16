//! Disk preservation only; retaining a tag does not implement its gameplay.

use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

// Ownership follows Entity::write_nbt, LivingEntity::write_living_nbt and
// Player::write_custom_nbt, including optional fields and consumed legacy aliases.
// Never derive this set from one snapshot: absence can mean intentional removal.
const OWNED: &[&str] = &[
    "id",
    "UUID",
    "Pos",
    "Motion",
    "Rotation",
    "Fire",
    "OnGround",
    "Invulnerable",
    "PortalCooldown",
    "HasVisualFire",
    "TicksFrozen",
    "CustomName",
    "CustomNameVisible",
    "Tags",
    "PumpkinCustomData",
    "BukkitValues",
    "Health",
    "AbsorptionAmount",
    "FallDistance",
    "fall_distance",
    "HurtTime",
    "DeathTime",
    "FallFlying",
    "active_effects",
    "equipment",
    "DataVersion",
    "SelectedItemSlot",
    "Inventory",
    "EnderItems",
    "abilities",
    "XpP",
    "XpLevel",
    "XpTotal",
    "XpSeed",
    "Score",
    "SleepTimer",
    "playerGameType",
    "previousPlayerGameType",
    "seenCredits",
    "warden_spawn_tracker",
    "spawn_extra_particles_on_fall",
    "HasPlayedBefore",
    "foodLevel",
    "foodSaturationLevel",
    "foodExhaustionLevel",
    "foodTickTimer",
    "Air",
    "AirSupply",
    "DrowningTick",
    "Dimension",
    "SpawnX",
    "SpawnY",
    "SpawnZ",
    "SpawnDimension",
    "SpawnForced",
    "SpawnAngle",
    "respawn",
    "RootVehicle",
    "Statistics",
];

const PARTIAL: &[(&str, &[&str])] = &[
    (
        "abilities",
        &[
            "invulnerable",
            "flying",
            "mayfly",
            "instabuild",
            "mayBuild",
            "flySpeed",
            "walkSpeed",
        ],
    ),
    (
        "warden_spawn_tracker",
        &[
            "ticks_since_last_warning",
            "warning_level",
            "cooldown_ticks",
        ],
    ),
    (
        "respawn",
        &["dimension", "pos", "yaw", "pitch", "angle", "forced"],
    ),
    (
        "equipment",
        &[
            "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
        ],
    ),
];

#[derive(Default)]
pub(super) struct PreservedPlayerData {
    top_level: NbtCompound,
    compounds: NbtCompound,
    // TextComponent can normalize or omit extensions. Keep the original tag
    // while the modeled name is unchanged, but never restore a removed/replaced name.
    custom_name: Option<(NbtTag, NbtTag)>,
}

impl PreservedPlayerData {
    pub(super) fn capture(nbt: &NbtCompound, entity_data: &NbtCompound) -> Self {
        let mut top_level = NbtCompound::new();
        for (key, value) in &nbt.child_tags {
            if !OWNED.contains(&key.as_ref()) {
                top_level.put(key, value.clone());
            }
        }
        let mut compounds = NbtCompound::new();
        for &(key, children) in PARTIAL {
            if let Some(compound) = nbt.get_compound(key) {
                let mut remainder = NbtCompound::new();
                for (child, value) in &compound.child_tags {
                    if !children.contains(&child.as_ref()) {
                        remainder.put(child, value.clone());
                    }
                }
                if !remainder.is_empty() {
                    compounds.put_compound(key, remainder);
                }
            }
        }
        // Statistics owns and retains its complete private typed tree. Canonical
        // JSON has separate load eligibility and never rewrites those counters.
        Self {
            top_level,
            compounds,
            custom_name: nbt
                .get("CustomName")
                .zip(entity_data.get("CustomName"))
                .map(|(raw, modeled)| (raw.clone(), modeled.clone())),
        }
    }

    pub(super) fn merge_into(&self, modeled: &mut NbtCompound) {
        for (key, value) in &self.top_level.child_tags {
            // Current output wins even if a later serializer adds a formerly opaque key.
            if !modeled.child_tags.contains_key(key) {
                modeled.put(key, value.clone());
            }
        }
        for (key, remainder) in &self.compounds.child_tags {
            if let Some(NbtTag::Compound(current)) = modeled.child_tags.get_mut(key)
                && let NbtTag::Compound(remainder) = remainder
            {
                for (child, value) in &remainder.child_tags {
                    if !current.child_tags.contains_key(child) {
                        current.put(child, value.clone());
                    }
                }
            }
        }
        if let Some((raw, original)) = &self.custom_name
            && modeled.get("CustomName") == Some(original)
        {
            modeled.put("CustomName", raw.clone());
        }
    }
}

pub(super) fn fall_distance(nbt: &NbtCompound) -> Option<f64> {
    nbt.get_double("fall_distance")
        .or_else(|| nbt.get_float("fall_distance").map(f64::from))
        .or_else(|| nbt.get_float("FallDistance").map(f64::from))
}

#[cfg(test)]
mod tests;
