//! Typed damage registry keys and native JSON codecs.
use ferraloom_data::{datapack, registries::sound_event};
use serde_json::Value;

macro_rules! named_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
            pub fn decode(value: &Value) -> Result<Self, datapack::Error> {
                match value.as_str() {
                    $(Some($text) => Ok(Self::$variant),)+
                    _ => Err(datapack::Error::InvalidData(format!("invalid {}: {value}", stringify!($name)))),
                }
            }
        }
    };
}

named_enum! {
    /// Port of DamageScaling.
    DamageScaling {
        Never => "never",
        WhenCausedByLivingNonPlayer => "when_caused_by_living_non_player",
        Always => "always",
    }
}
named_enum! {
    /// Port of DamageEffects.
    DamageEffects {
        Hurt => "hurt", Thorns => "thorns", Drowning => "drowning",
        Burning => "burning", Poking => "poking", Freezing => "freezing",
    }
}
impl DamageEffects {
    pub fn sound(self) -> sound_event::Id {
        match self {
            Self::Hurt | Self::Thorns => sound_event::Id::ENTITY_PLAYER_HURT,
            Self::Drowning => sound_event::Id::ENTITY_PLAYER_HURT_DROWN,
            Self::Burning => sound_event::Id::ENTITY_PLAYER_HURT_ON_FIRE,
            Self::Poking => sound_event::Id::ENTITY_PLAYER_HURT_SWEET_BERRY_BUSH,
            Self::Freezing => sound_event::Id::ENTITY_PLAYER_HURT_FREEZE,
        }
    }
}
named_enum! {
    /// Port of DeathMessageType.
    DeathMessageType {
        Default => "default", FallVariants => "fall_variants",
        IntentionalGameDesign => "intentional_game_design",
    }
}

/// Port of DamageType. Values come from the actual damage_type datapack registry.
#[derive(Clone, Debug)]
pub struct DamageType {
    pub message_id: String,
    pub scaling: DamageScaling,
    pub exhaustion: f32,
    pub effects: DamageEffects,
    pub death_message_type: DeathMessageType,
}
impl DamageType {
    /// Port of DamageType#DIRECT_CODEC, including null optional fields and the
    /// unchecked FLOAT conversion (overflow to infinity is accepted by vanilla).
    pub fn decode(value: &Value) -> Result<Self, datapack::Error> {
        let invalid =
            |field| datapack::Error::InvalidData(format!("invalid damage type {field}: {value}"));
        let message_id = value
            .get("message_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("message_id"))?
            .to_owned();
        let scaling =
            DamageScaling::decode(value.get("scaling").ok_or_else(|| invalid("scaling"))?)?;
        let exhaustion = value
            .get("exhaustion")
            .and_then(Value::as_number)
            .ok_or_else(|| invalid("exhaustion"))?
            .to_string()
            .parse::<f32>()
            .map_err(|_| invalid("exhaustion"))?;
        let effects = match value.get("effects").filter(|v| !v.is_null()) {
            Some(value) => DamageEffects::decode(value)?,
            None => DamageEffects::Hurt,
        };
        let death_message_type = match value.get("death_message_type").filter(|v| !v.is_null()) {
            Some(value) => DeathMessageType::decode(value)?,
            None => DeathMessageType::Default,
        };
        Ok(Self {
            message_id,
            scaling,
            exhaustion,
            effects,
            death_message_type,
        })
    }
}

named_enum! {
    /// Port of DamageTypes resource keys. These ordinals are local cache slots,
    /// never network registry IDs. Datapack-defined keys are resolved separately.
    DamageTypeKey {
        InFire => "minecraft:in_fire",
        Campfire => "minecraft:campfire",
        LightningBolt => "minecraft:lightning_bolt",
        OnFire => "minecraft:on_fire",
        Lava => "minecraft:lava",
        HotFloor => "minecraft:hot_floor",
        SulfurCubeHot => "minecraft:sulfur_cube_hot",
        InWall => "minecraft:in_wall",
        Cramming => "minecraft:cramming",
        Drown => "minecraft:drown",
        Starve => "minecraft:starve",
        Cactus => "minecraft:cactus",
        Fall => "minecraft:fall",
        EnderPearl => "minecraft:ender_pearl",
        FlyIntoWall => "minecraft:fly_into_wall",
        FellOutOfWorld => "minecraft:out_of_world",
        Generic => "minecraft:generic",
        Magic => "minecraft:magic",
        Wither => "minecraft:wither",
        DragonBreath => "minecraft:dragon_breath",
        DryOut => "minecraft:dry_out",
        SweetBerryBush => "minecraft:sweet_berry_bush",
        Freeze => "minecraft:freeze",
        Stalagmite => "minecraft:stalagmite",
        FallingBlock => "minecraft:falling_block",
        FallingAnvil => "minecraft:falling_anvil",
        FallingStalactite => "minecraft:falling_stalactite",
        Sting => "minecraft:sting",
        MobAttack => "minecraft:mob_attack",
        MobAttackNoAggro => "minecraft:mob_attack_no_aggro",
        PlayerAttack => "minecraft:player_attack",
        Spear => "minecraft:spear",
        Arrow => "minecraft:arrow",
        Trident => "minecraft:trident",
        MobProjectile => "minecraft:mob_projectile",
        Spit => "minecraft:spit",
        WindCharge => "minecraft:wind_charge",
        Fireworks => "minecraft:fireworks",
        Fireball => "minecraft:fireball",
        UnattributedFireball => "minecraft:unattributed_fireball",
        WitherSkull => "minecraft:wither_skull",
        Thrown => "minecraft:thrown",
        IndirectMagic => "minecraft:indirect_magic",
        Thorns => "minecraft:thorns",
        Explosion => "minecraft:explosion",
        PlayerExplosion => "minecraft:player_explosion",
        SonicBoom => "minecraft:sonic_boom",
        BadRespawnPoint => "minecraft:bad_respawn_point",
        OutsideBorder => "minecraft:outside_border",
        GenericKill => "minecraft:generic_kill",
        MaceSmash => "minecraft:mace_smash",
    }
}

named_enum! {
    /// Port of DamageTypeTags. Membership is resolved from the loaded tag set.
    DamageTypeTag {
        DamagesHelmet => "minecraft:damages_helmet",
        BypassesArmor => "minecraft:bypasses_armor",
        BypassesShield => "minecraft:bypasses_shield",
        BypassesInvulnerability => "minecraft:bypasses_invulnerability",
        BypassesCooldown => "minecraft:bypasses_cooldown",
        BypassesEffects => "minecraft:bypasses_effects",
        BypassesResistance => "minecraft:bypasses_resistance",
        BypassesEnchantments => "minecraft:bypasses_enchantments",
        IsFire => "minecraft:is_fire",
        IsProjectile => "minecraft:is_projectile",
        WitchResistantTo => "minecraft:witch_resistant_to",
        IsExplosion => "minecraft:is_explosion",
        IsFall => "minecraft:is_fall",
        IsDrowning => "minecraft:is_drowning",
        IsFreezing => "minecraft:is_freezing",
        IsLightning => "minecraft:is_lightning",
        NoAnger => "minecraft:no_anger",
        NoWolfRetaliation => "minecraft:no_wolf_retaliation",
        NoImpact => "minecraft:no_impact",
        AlwaysMostSignificantFall => "minecraft:always_most_significant_fall",
        WitherImmuneTo => "minecraft:wither_immune_to",
        SulfurCubeWithBlockImmuneTo => "minecraft:sulfur_cube_with_block_immune_to",
        IgnitesArmorStands => "minecraft:ignites_armor_stands",
        BurnsArmorStands => "minecraft:burns_armor_stands",
        AvoidsGuardianThorns => "minecraft:avoids_guardian_thorns",
        AlwaysTriggersSilverfish => "minecraft:always_triggers_silverfish",
        AlwaysHurtsEnderDragons => "minecraft:always_hurts_ender_dragons",
        NoKnockback => "minecraft:no_knockback",
        AlwaysKillsArmorStands => "minecraft:always_kills_armor_stands",
        CanBreakArmorStand => "minecraft:can_break_armor_stand",
        BypassesWolfArmor => "minecraft:bypasses_wolf_armor",
        IsPlayerAttack => "minecraft:is_player_attack",
        BurnFromStepping => "minecraft:burn_from_stepping",
        PanicCauses => "minecraft:panic_causes",
        PanicEnvironmentalCauses => "minecraft:panic_environmental_causes",
        IsMaceSmash => "minecraft:mace_smash",
    }
}
