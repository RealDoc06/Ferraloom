//! `MipmapGenerator`: the mip levels of a sprite. Cutout textures keep the
//! alpha-test coverage of their base level at every level
//! (`scaleAlphaToCoverage`), so thin cutout edges (cactus sides, leaves,
//! flowers) don't erode into gaps with distance; colours average in linear
//! light (`ARGB.meanLinear`).

/// `MipmapStrategy`, from the texture's `.mcmeta` `texture` section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Strategy {
    #[default]
    Auto,
    Mean,
    Cutout,
    StrictCutout,
    DarkCutout,
}
impl Strategy {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "auto" => Self::Auto,
            "mean" => Self::Mean,
            "cutout" => Self::Cutout,
            "strict_cutout" => Self::StrictCutout,
            "dark_cutout" => Self::DarkCutout,
            _ => return None,
        })
    }
}

/// How a sprite's mips are made: its `.mcmeta` `texture` section, whether
/// it has a fully transparent pixel and whether it is an `item/` sprite.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Settings {
    /// Original square frame edge, before fitting the sprite into the array.
    /// Zero means the supplied image is already at its native size.
    pub native_size: u32,
    pub strategy: Strategy,
    pub alpha_cutoff_bias: f32,
    pub has_transparent: bool,
    pub item: bool,
}
