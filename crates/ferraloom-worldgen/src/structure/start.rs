//! StructureStart and piece persistence. Chunk storage owns the containing NBT;
//! registry resolution and piece codecs remain in world generation.
use super::{
    BoundingBox, JigsawJunction, Projection, TerrainAdjustment,
    jigsaw::{JigsawStart, PoolElementStructurePiece},
    nbt::{sequence, string},
    pool::StructurePoolElement,
    template::{LiquidSettings, StructureTemplateManager},
};
use crate::{Error, Identifier, feature::PlacedFeature};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Direction, Rotation};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

/// The ported piece variants. Handwritten variants are added with their actual
/// generators and postProcess implementations; unknown pieces never become air.
#[derive(Clone, Debug)]
pub enum StructurePiece {
    Jigsaw(PoolElementStructurePiece),
    BuriedTreasure(super::structures::buried_treasure::BuriedTreasurePiece),
    Igloo(Box<super::structures::igloo::IglooPiece>),
    Shipwreck(Box<super::structures::shipwreck::ShipwreckPiece>),
    NetherFossil(Box<super::structures::nether_fossil::NetherFossilPiece>),
    EndCity(Box<super::structures::end_city::EndCityPiece>),
    OceanRuin(Box<super::structures::ocean_ruin::OceanRuinPiece>),
}
impl StructurePiece {
    pub fn terrain_piece(&self, adjustment: TerrainAdjustment) -> super::TerrainPiece<'_> {
        match self {
            Self::Jigsaw(piece) => super::TerrainPiece {
                bounds: piece.bounds,
                adjustment,
                ground_level_delta: piece.ground_level_delta,
                projection: Some(piece.element.projection()),
                junctions: &piece.junctions,
            },
            _ => super::TerrainPiece {
                bounds: self.bounds(),
                adjustment,
                ground_level_delta: 0,
                projection: None,
                junctions: &[],
            },
        }
    }
    pub fn bounds(&self) -> BoundingBox {
        match self {
            Self::Jigsaw(p) => p.bounds,
            Self::BuriedTreasure(p) => p.data.bounds,
            Self::Igloo(p) => p.template.data.bounds,
            Self::Shipwreck(p) => p.template.data.bounds,
            Self::NetherFossil(p) => p.template.data.bounds,
            Self::EndCity(p) => p.template.data.bounds,
            Self::OceanRuin(p) => p.template.data.bounds,
        }
    }
    pub fn to_nbt(&self) -> Result<CompoundTag, Error> {
        match self {
            Self::Jigsaw(p) => p.to_nbt(),
            Self::BuriedTreasure(p) => Ok(p.to_nbt()),
            Self::Igloo(p) => Ok(p.to_nbt()),
            Self::Shipwreck(p) => Ok(p.to_nbt()),
            Self::NetherFossil(p) => Ok(p.to_nbt()),
            Self::EndCity(p) => Ok(p.to_nbt()),
            Self::OceanRuin(p) => Ok(p.to_nbt()),
        }
    }
}
/// Port of StructureStart. The bounding-box cache survives piece mutation, as
/// vanilla's cache does. The owning chunk exclusively borrows mutable starts.
#[derive(Clone, Debug)]
pub struct StructureStart {
    pub structure: Option<Identifier>,
    pub chunk: [i32; 2],
    pub references: i32,
    pub pieces: Vec<StructurePiece>,
    pub terrain: TerrainAdjustment,
    cached_bounds: OnceLock<BoundingBox>,
}
impl StructureStart {
    pub fn invalid() -> Self {
        Self {
            structure: None,
            chunk: [0; 2],
            references: 0,
            pieces: Vec::new(),
            terrain: TerrainAdjustment::None,
            cached_bounds: OnceLock::new(),
        }
    }
    pub fn new(
        structure: Identifier,
        chunk: [i32; 2],
        references: i32,
        terrain: TerrainAdjustment,
        pieces: Vec<StructurePiece>,
    ) -> Self {
        Self {
            structure: Some(structure),
            chunk,
            references,
            terrain,
            pieces,
            cached_bounds: OnceLock::new(),
        }
    }
    pub fn from_jigsaw(
        structure: Identifier,
        chunk: [i32; 2],
        references: i32,
        terrain: TerrainAdjustment,
        start: JigsawStart,
    ) -> Self {
        Self::new(
            structure,
            chunk,
            references,
            terrain,
            start
                .pieces
                .into_iter()
                .map(StructurePiece::Jigsaw)
                .collect(),
        )
    }
    pub fn is_valid(&self) -> bool {
        !self.pieces.is_empty()
    }
    pub fn can_be_referenced(&self) -> bool {
        self.references < 1
    }
    pub fn add_reference(&mut self) {
        self.references = self.references.wrapping_add(1);
    }
    pub fn bounding_box(&self) -> Result<BoundingBox, Error> {
        if let Some(&bounds) = self.cached_bounds.get() {
            return Ok(bounds);
        }
        let bounds = self
            .pieces
            .iter()
            .map(StructurePiece::bounds)
            .reduce(BoundingBox::encapsulating)
            .ok_or_else(|| Error("Unable to calculate boundingbox without pieces".into()))?;
        let bounds = if self.terrain == TerrainAdjustment::None {
            bounds
        } else {
            bounds.inflated(12)
        };
        Ok(*self.cached_bounds.get_or_init(|| bounds))
    }
    /// Port of StructureStart#createTag. The chunk coordinate argument is the
    /// storing chunk's position, rather than the start's retained source field.
    pub fn to_nbt(&self, chunk: [i32; 2]) -> Result<CompoundTag, Error> {
        let mut tag = CompoundTag::new();
        if !self.is_valid() {
            tag.insert("id", string("INVALID"));
            return Ok(tag);
        }
        tag.insert(
            "id",
            string(
                self.structure
                    .as_ref()
                    .ok_or_else(|| Error("valid start has no structure".into()))?,
            ),
        );
        tag.insert("ChunkX", Tag::Int(chunk[0]));
        tag.insert("ChunkZ", Tag::Int(chunk[1]));
        tag.insert("references", Tag::Int(self.references));
        tag.insert(
            "Children",
            sequence(
                self.pieces
                    .iter()
                    .map(|p| p.to_nbt().map(Tag::Compound))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
        Ok(tag)
    }
    /// Current-version start codec. Old chunks pass through storage's data
    /// fixer before this call. Unknown structure ids are discarded like vanilla.
    pub fn from_nbt(
        tag: &CompoundTag,
        settings: &BTreeMap<Identifier, TerrainAdjustment>,
        templates: &StructureTemplateManager,
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&serde_json::Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Option<Self>, Error> {
        let Some(id) = tag.get_string("id") else {
            return Ok(None);
        };
        if id.to_string_lossy() == "INVALID" {
            return Ok(Some(Self::invalid()));
        }
        let Ok(id) = id.to_string_lossy().parse::<Identifier>() else {
            return Ok(None);
        };
        let Some(&terrain) = settings.get(&id) else {
            return Ok(None);
        };
        let mut pieces = Vec::new();
        if let Some(children) = tag.get("Children").and_then(Tag::as_list) {
            for tag in children.iter().filter_map(Tag::as_compound) {
                let name = tag
                    .get_string("id")
                    .map(|s| s.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let Ok(piece_id) = name.parse::<Identifier>() else {
                    return Ok(None);
                };
                match piece_id.to_string().as_str() {
                    "minecraft:orp" => {
                        if let Ok(piece) =
                            super::structures::ocean_ruin::OceanRuinPiece::from_nbt(templates, tag)
                        {
                            pieces.push(StructurePiece::OceanRuin(Box::new(piece)));
                        }
                    }
                    "minecraft:ecp" => {
                        if let Ok(piece) =
                            super::structures::end_city::EndCityPiece::from_nbt(templates, tag)
                        {
                            pieces.push(StructurePiece::EndCity(Box::new(piece)));
                        }
                    }
                    "minecraft:nefos" => {
                        if let Ok(piece) =
                            super::structures::nether_fossil::NetherFossilPiece::from_nbt(
                                templates, tag,
                            )
                        {
                            pieces.push(StructurePiece::NetherFossil(Box::new(piece)));
                        }
                    }
                    "minecraft:shipwreck" => {
                        if let Ok(piece) =
                            super::structures::shipwreck::ShipwreckPiece::from_nbt(templates, tag)
                        {
                            pieces.push(StructurePiece::Shipwreck(Box::new(piece)));
                        }
                    }
                    "minecraft:iglu" => {
                        if let Ok(piece) =
                            super::structures::igloo::IglooPiece::from_nbt(templates, tag)
                        {
                            pieces.push(StructurePiece::Igloo(Box::new(piece)));
                        }
                    }
                    "minecraft:btp" => {
                        if let Ok(piece) =
                            super::structures::buried_treasure::BuriedTreasurePiece::from_nbt(tag)
                        {
                            pieces.push(StructurePiece::BuriedTreasure(piece));
                        }
                    }
                    "minecraft:jigsaw"
                    | "minecraft:nvi"
                    | "minecraft:pcp"
                    | "minecraft:bastionremnant"
                    | "minecraft:runtime" => {
                        if let Ok(piece) = PoolElementStructurePiece::from_nbt(
                            tag,
                            templates,
                            resources,
                            decode_feature,
                        ) {
                            pieces.push(StructurePiece::Jigsaw(piece));
                        }
                    }
                    // Registered handwritten pieces gain typed decoding with
                    // their ports; do not silently discard unimplemented data.
                    _ if ferraloom_data::registries::worldgen_structure_piece::Id::from_name(
                        &piece_id.to_string(),
                    )
                    .is_some() =>
                    {
                        return Err(Error(format!("Unsupported structure piece {piece_id}")));
                    }
                    _ => {}
                }
            }
        }
        Ok(Some(Self::new(
            id,
            [
                tag.get_int("ChunkX").unwrap_or(0),
                tag.get_int("ChunkZ").unwrap_or(0),
            ],
            tag.get_int("references").unwrap_or(0),
            terrain,
            pieces,
        )))
    }
}
impl PoolElementStructurePiece {
    pub fn to_nbt(&self) -> Result<CompoundTag, Error> {
        let mut tag = CompoundTag::new();
        tag.insert("id", string("minecraft:jigsaw"));
        tag.insert(
            "BB",
            Tag::IntArray(self.bounds.min.into_iter().chain(self.bounds.max).collect()),
        );
        tag.insert("GD", Tag::Int(self.gen_depth));
        tag.insert(
            "O",
            Tag::Int(self.orientation.map_or(-1, Direction::data_2d)),
        );
        for (key, value) in [
            ("PosX", self.position[0]),
            ("PosY", self.position[1]),
            ("PosZ", self.position[2]),
            ("ground_level_delta", self.ground_level_delta),
        ] {
            tag.insert(key, Tag::Int(value));
        }
        tag.insert(
            "rotation",
            string(match self.rotation {
                Rotation::None => "NONE",
                Rotation::Clockwise90 => "CLOCKWISE_90",
                Rotation::Clockwise180 => "CLOCKWISE_180",
                Rotation::CounterClockwise90 => "COUNTERCLOCKWISE_90",
            }),
        );
        tag.insert("pool_element", self.element.to_nbt()?);
        tag.insert(
            "junctions",
            sequence(self.junctions.iter().map(|j| {
                super::nbt::compound([
                    ("source_x", Tag::Int(j.source[0])),
                    ("source_ground_y", Tag::Int(j.source[1])),
                    ("source_z", Tag::Int(j.source[2])),
                    ("delta_y", Tag::Int(j.delta_y)),
                    (
                        "dest_proj",
                        string(if j.destination_projection == Projection::Rigid {
                            "rigid"
                        } else {
                            "terrain_matching"
                        }),
                    ),
                ])
            })),
        );
        if self.liquid != LiquidSettings::ApplyWaterlogging {
            tag.insert("liquid_settings", string("ignore_waterlogging"));
        }
        Ok(tag)
    }
    pub fn from_nbt(
        tag: &CompoundTag,
        templates: &StructureTemplateManager,
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&serde_json::Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        if tag
            .get("BB")
            .and_then(Tag::as_int_array)
            .is_none_or(|v| v.len() != 6)
        {
            return Err(Error("invalid piece bounding box".into()));
        }
        let rotation = match tag
            .get_string("rotation")
            .map(|s| s.to_string_lossy())
            .as_deref()
        {
            Some("NONE") => Rotation::None,
            Some("CLOCKWISE_90") => Rotation::Clockwise90,
            Some("CLOCKWISE_180") => Rotation::Clockwise180,
            Some("COUNTERCLOCKWISE_90") => Rotation::CounterClockwise90,
            _ => return Err(Error("invalid piece rotation".into())),
        };
        let element = Arc::new(StructurePoolElement::decode(
            &super::nbt::to_json(
                tag.get("pool_element")
                    .ok_or_else(|| Error("missing pool element".into()))?,
            )?,
            resources,
            decode_feature,
        )?);
        let position = [
            tag.get_int("PosX").unwrap_or(0),
            tag.get_int("PosY").unwrap_or(0),
            tag.get_int("PosZ").unwrap_or(0),
        ];
        let mut junctions = Vec::new();
        if let Some(values) = tag.get("junctions").and_then(Tag::as_list) {
            for value in values.iter() {
                let value = value
                    .as_compound()
                    .ok_or_else(|| Error("invalid jigsaw junction".into()))?;
                junctions.push(JigsawJunction {
                    source: [
                        value.get_int("source_x").unwrap_or(0),
                        value.get_int("source_ground_y").unwrap_or(0),
                        value.get_int("source_z").unwrap_or(0),
                    ],
                    delta_y: value.get_int("delta_y").unwrap_or(0),
                    destination_projection: Projection::decode(&serde_json::Value::String(
                        value
                            .get_string("dest_proj")
                            .map(|s| s.to_string_lossy())
                            .unwrap_or_default(),
                    ))?,
                });
            }
        }
        Ok(Self {
            bounds: element.bounding_box(templates, position, rotation)?,
            element,
            position,
            rotation,
            ground_level_delta: tag.get_int("ground_level_delta").unwrap_or(0),
            liquid: tag
                .get("liquid_settings")
                .and_then(|v| super::nbt::to_json(v).ok())
                .and_then(|v| LiquidSettings::decode(&v).ok())
                .unwrap_or_default(),
            gen_depth: tag.get_int("GD").unwrap_or(0),
            orientation: match tag.get_int("O").unwrap_or(0) {
                -1 => None,
                value => Some(Direction::from_2d_data_value(value)),
            },
            junctions,
        })
    }
}
