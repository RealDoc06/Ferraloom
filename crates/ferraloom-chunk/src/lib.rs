//! Vanilla 26.3 chunk containers shared by simulation, worldgen, IO and protocol.
//! No NBT or wire codecs live here. Raw memory and packed palette views are public.

mod bit_storage;
mod palette;
mod section;

pub use bit_storage::{BitStorage, SimpleBitStorage, ZeroBitStorage};
pub use palette::{
    Configuration, ContainerKind, IdMap, PackedData, PaletteKind, PalettedContainer, Strategy,
};
pub use section::{BiomeId, LevelChunkSection, SectionCounts};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContainerError {
    InvalidBits(u8),
    InvalidLength { expected: usize, actual: usize },
    EmptyRegistry,
    DuplicateRegistryValue,
    EmptyPalette,
    PaletteTooLarge,
    UnknownRegistryValue,
    MissingStorage,
    DeclaredBits { expected: u8, actual: u8 },
}

#[cfg(test)]
mod tests;
