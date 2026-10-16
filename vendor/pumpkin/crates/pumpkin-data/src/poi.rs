//! Static point-of-interest definitions pinned to Minecraft 26.3.
//!
//! This catalog only classifies block states. Persistent records, ticket accounting,
//! section reconciliation and runtime POI updates belong to its callers.

use crate::{
    BlockId, BlockStateId,
    block_properties::{BedPart, BedProperties},
};

/// A registered POI type, including its stable identifier and registration parameters.
/// See the test fixture for the pinned bytecode and block-report provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoiType {
    Armorer,
    Butcher,
    Cartographer,
    Cleric,
    Farmer,
    Fisherman,
    Fletcher,
    Leatherworker,
    Librarian,
    Mason,
    Shepherd,
    Toolsmith,
    Weaponsmith,
    Home,
    Meeting,
    Beehive,
    BeeNest,
    NetherPortal,
    Lodestone,
    TestInstance,
    LightningRod,
}

impl PoiType {
    pub const ALL: [Self; 21] = [
        Self::Armorer,
        Self::Butcher,
        Self::Cartographer,
        Self::Cleric,
        Self::Farmer,
        Self::Fisherman,
        Self::Fletcher,
        Self::Leatherworker,
        Self::Librarian,
        Self::Mason,
        Self::Shepherd,
        Self::Toolsmith,
        Self::Weaponsmith,
        Self::Home,
        Self::Meeting,
        Self::Beehive,
        Self::BeeNest,
        Self::NetherPortal,
        Self::Lodestone,
        Self::TestInstance,
        Self::LightningRod,
    ];

    #[must_use]
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Armorer => "minecraft:armorer",
            Self::Butcher => "minecraft:butcher",
            Self::Cartographer => "minecraft:cartographer",
            Self::Cleric => "minecraft:cleric",
            Self::Farmer => "minecraft:farmer",
            Self::Fisherman => "minecraft:fisherman",
            Self::Fletcher => "minecraft:fletcher",
            Self::Leatherworker => "minecraft:leatherworker",
            Self::Librarian => "minecraft:librarian",
            Self::Mason => "minecraft:mason",
            Self::Shepherd => "minecraft:shepherd",
            Self::Toolsmith => "minecraft:toolsmith",
            Self::Weaponsmith => "minecraft:weaponsmith",
            Self::Home => "minecraft:home",
            Self::Meeting => "minecraft:meeting",
            Self::Beehive => "minecraft:beehive",
            Self::BeeNest => "minecraft:bee_nest",
            Self::NetherPortal => "minecraft:nether_portal",
            Self::Lodestone => "minecraft:lodestone",
            Self::TestInstance => "minecraft:test_instance",
            Self::LightningRod => "minecraft:lightning_rod",
        }
    }

    #[must_use]
    pub const fn max_tickets(self) -> i32 {
        match self {
            Self::Meeting => 32,
            Self::Beehive
            | Self::BeeNest
            | Self::NetherPortal
            | Self::Lodestone
            | Self::TestInstance
            | Self::LightningRod => 0,
            Self::Armorer
            | Self::Butcher
            | Self::Cartographer
            | Self::Cleric
            | Self::Farmer
            | Self::Fisherman
            | Self::Fletcher
            | Self::Leatherworker
            | Self::Librarian
            | Self::Mason
            | Self::Shepherd
            | Self::Toolsmith
            | Self::Weaponsmith
            | Self::Home => 1,
        }
    }

    #[must_use]
    pub const fn valid_range(self) -> i32 {
        match self {
            Self::Meeting => 6,
            _ => 1,
        }
    }

    /// Classifies a state by block identity and, for the sixteen colored beds,
    /// the generated HEAD property. Straw beds are not registered as HOME.
    #[must_use]
    pub fn from_state(state: BlockStateId) -> Option<Self> {
        Some(match state.to_block_id() {
            BlockId::BLAST_FURNACE => Self::Armorer,
            BlockId::SMOKER => Self::Butcher,
            BlockId::CARTOGRAPHY_TABLE => Self::Cartographer,
            BlockId::BREWING_STAND => Self::Cleric,
            BlockId::COMPOSTER => Self::Farmer,
            BlockId::BARREL => Self::Fisherman,
            BlockId::FLETCHING_TABLE => Self::Fletcher,
            BlockId::CAULDRON
            | BlockId::WATER_CAULDRON
            | BlockId::LAVA_CAULDRON
            | BlockId::POWDER_SNOW_CAULDRON => Self::Leatherworker,
            BlockId::LECTERN => Self::Librarian,
            BlockId::STONECUTTER => Self::Mason,
            BlockId::LOOM => Self::Shepherd,
            BlockId::SMITHING_TABLE => Self::Toolsmith,
            BlockId::GRINDSTONE => Self::Weaponsmith,
            BlockId::WHITE_BED
            | BlockId::ORANGE_BED
            | BlockId::MAGENTA_BED
            | BlockId::LIGHT_BLUE_BED
            | BlockId::YELLOW_BED
            | BlockId::LIME_BED
            | BlockId::PINK_BED
            | BlockId::GRAY_BED
            | BlockId::LIGHT_GRAY_BED
            | BlockId::CYAN_BED
            | BlockId::PURPLE_BED
            | BlockId::BLUE_BED
            | BlockId::BROWN_BED
            | BlockId::GREEN_BED
            | BlockId::RED_BED
            | BlockId::BLACK_BED => {
                if BedProperties::from_state_id(state).part != BedPart::Head {
                    return None;
                }
                Self::Home
            }
            BlockId::BELL => Self::Meeting,
            BlockId::BEEHIVE => Self::Beehive,
            BlockId::BEE_NEST => Self::BeeNest,
            BlockId::NETHER_PORTAL => Self::NetherPortal,
            BlockId::LODESTONE => Self::Lodestone,
            BlockId::TEST_INSTANCE_BLOCK => Self::TestInstance,
            BlockId::LIGHTNING_ROD
            | BlockId::EXPOSED_LIGHTNING_ROD
            | BlockId::WEATHERED_LIGHTNING_ROD
            | BlockId::OXIDIZED_LIGHTNING_ROD
            | BlockId::WAXED_LIGHTNING_ROD
            | BlockId::WAXED_EXPOSED_LIGHTNING_ROD
            | BlockId::WAXED_WEATHERED_LIGHTNING_ROD
            | BlockId::WAXED_OXIDIZED_LIGHTNING_ROD => Self::LightningRod,
            _ => return None,
        })
    }
}

#[cfg(test)]
#[path = "poi/tests.rs"]
mod tests;
