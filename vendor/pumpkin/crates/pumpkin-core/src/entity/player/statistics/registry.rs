//! Core-owned 26.3 adapter. `Stats.<clinit>` offsets 0..96 bind the value registries:
//! mined -> BLOCK; crafted/used/broken/picked_up/dropped -> ITEM;
//! killed/killed_by -> ENTITY_TYPE; custom -> CUSTOM_STAT.
//! Primary source: .cache/vanilla/26.3/minecraft.jar, net.minecraft.stats.Stats.
//! Jar SHA256: a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6.
//! Registry witness: registry-witness.json (all IDs/names, no copied registry blob).

use super::{CustomStatistic, StatisticCategory};
use pumpkin_data::{Block, BlockId, entity::EntityType, item::Item};

pub(super) fn category(name: &str) -> Option<StatisticCategory> {
    let key = name.strip_prefix("minecraft:")?;
    if key.contains(':') {
        return None;
    }
    StatisticCategory::from_registry_key(key)
}

pub(super) const fn category_name(category: StatisticCategory) -> &'static str {
    match category {
        StatisticCategory::Mined => "minecraft:mined",
        StatisticCategory::Crafted => "minecraft:crafted",
        StatisticCategory::Used => "minecraft:used",
        StatisticCategory::Broken => "minecraft:broken",
        StatisticCategory::PickedUp => "minecraft:picked_up",
        StatisticCategory::Dropped => "minecraft:dropped",
        StatisticCategory::Killed => "minecraft:killed",
        StatisticCategory::KilledBy => "minecraft:killed_by",
        StatisticCategory::Custom => "minecraft:custom",
    }
}

pub(super) fn stat_key(category: StatisticCategory, id: i32) -> Option<&'static str> {
    // Conversion and BlockId::new must precede Block::from_id (unchecked internally).
    let raw = u16::try_from(id).ok()?;
    match category {
        StatisticCategory::Mined => Some(Block::from_id(BlockId::new(raw)?).name),
        StatisticCategory::Crafted
        | StatisticCategory::Used
        | StatisticCategory::Broken
        | StatisticCategory::PickedUp
        | StatisticCategory::Dropped => Some(Item::from_id(raw)?.registry_key),
        StatisticCategory::Killed | StatisticCategory::KilledBy => {
            Some(EntityType::from_raw(raw)?.resource_name)
        }
        StatisticCategory::Custom => Some(custom_key(CustomStatistic::from_i32(id)?)),
    }
}

pub(super) fn stat_id(category: StatisticCategory, name: &str) -> Option<i32> {
    let key = name.strip_prefix("minecraft:")?;
    if key.contains(':') {
        return None;
    }
    match category {
        StatisticCategory::Mined => Some(i32::from(Block::from_registry_key(key)?.id.as_u16())),
        StatisticCategory::Crafted
        | StatisticCategory::Used
        | StatisticCategory::Broken
        | StatisticCategory::PickedUp
        | StatisticCategory::Dropped => Some(i32::from(Item::from_registry_key(key)?.id)),
        StatisticCategory::Killed | StatisticCategory::KilledBy => {
            Some(i32::from(EntityType::from_name(key)?.id))
        }
        StatisticCategory::Custom => Some(custom_from_key(key)? as i32),
    }
}

// Explicit identifiers, never Debug-derived spellings. The exhaustive enum match
// forces review when the generated custom-stat registry gains a variant.
macro_rules! custom_statistics {
    ($($variant:ident => $key:literal),* $(,)?) => {
        const fn custom_key(stat: CustomStatistic) -> &'static str {
            match stat { $(CustomStatistic::$variant => $key),* }
        }
        fn custom_from_key(key: &str) -> Option<CustomStatistic> {
            match key { $($key => Some(CustomStatistic::$variant),)* _ => None }
        }
    };
}

custom_statistics! {
    LeaveGame => "leave_game",
    PlayTime => "play_time",
    TotalWorldTime => "total_world_time",
    TimeSinceDeath => "time_since_death",
    TimeSinceRest => "time_since_rest",
    SneakTime => "sneak_time",
    WalkOneCm => "walk_one_cm",
    CrouchOneCm => "crouch_one_cm",
    SprintOneCm => "sprint_one_cm",
    WalkOnWaterOneCm => "walk_on_water_one_cm",
    FallOneCm => "fall_one_cm",
    ClimbOneCm => "climb_one_cm",
    FlyOneCm => "fly_one_cm",
    WalkUnderWaterOneCm => "walk_under_water_one_cm",
    MinecartOneCm => "minecart_one_cm",
    BoatOneCm => "boat_one_cm",
    PigOneCm => "pig_one_cm",
    HappyGhastOneCm => "happy_ghast_one_cm",
    HorseOneCm => "horse_one_cm",
    AviateOneCm => "aviate_one_cm",
    SwimOneCm => "swim_one_cm",
    StriderOneCm => "strider_one_cm",
    NautilusOneCm => "nautilus_one_cm",
    Jump => "jump",
    Drop => "drop",
    DamageDealt => "damage_dealt",
    DamageDealtAbsorbed => "damage_dealt_absorbed",
    DamageDealtResisted => "damage_dealt_resisted",
    DamageTaken => "damage_taken",
    DamageBlockedByShield => "damage_blocked_by_shield",
    DamageAbsorbed => "damage_absorbed",
    DamageResisted => "damage_resisted",
    Deaths => "deaths",
    MobKills => "mob_kills",
    AnimalsBred => "animals_bred",
    PlayerKills => "player_kills",
    FishCaught => "fish_caught",
    TalkedToVillager => "talked_to_villager",
    TradedWithVillager => "traded_with_villager",
    EatCakeSlice => "eat_cake_slice",
    FillCauldron => "fill_cauldron",
    UseCauldron => "use_cauldron",
    CleanArmor => "clean_armor",
    CleanBanner => "clean_banner",
    CleanShulkerBox => "clean_shulker_box",
    InteractWithBrewingstand => "interact_with_brewingstand",
    InteractWithBeacon => "interact_with_beacon",
    InspectDropper => "inspect_dropper",
    InspectHopper => "inspect_hopper",
    InspectDispenser => "inspect_dispenser",
    PlayNoteblock => "play_noteblock",
    TuneNoteblock => "tune_noteblock",
    PotFlower => "pot_flower",
    TriggerTrappedChest => "trigger_trapped_chest",
    OpenEnderchest => "open_enderchest",
    EnchantItem => "enchant_item",
    PlayRecord => "play_record",
    InteractWithFurnace => "interact_with_furnace",
    InteractWithCraftingTable => "interact_with_crafting_table",
    OpenChest => "open_chest",
    SleepInBed => "sleep_in_bed",
    SleepInStrawBed => "sleep_in_straw_bed",
    OpenShulkerBox => "open_shulker_box",
    OpenBarrel => "open_barrel",
    InteractWithBlastFurnace => "interact_with_blast_furnace",
    InteractWithSmoker => "interact_with_smoker",
    InteractWithLectern => "interact_with_lectern",
    InteractWithCampfire => "interact_with_campfire",
    InteractWithCartographyTable => "interact_with_cartography_table",
    InteractWithLoom => "interact_with_loom",
    InteractWithStonecutter => "interact_with_stonecutter",
    BellRing => "bell_ring",
    RaidTrigger => "raid_trigger",
    RaidWin => "raid_win",
    InteractWithAnvil => "interact_with_anvil",
    InteractWithGrindstone => "interact_with_grindstone",
    TargetHit => "target_hit",
    InteractWithSmithingTable => "interact_with_smithing_table",
}
