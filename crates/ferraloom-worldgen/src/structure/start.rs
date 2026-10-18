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
use ferraloom_data::{datapack::ResourceManager, registries::worldgen_structure_type};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Direction, Rotation};
use std::sync::{Arc, OnceLock};

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
    Stronghold(super::structures::stronghold::StrongholdPiece),
    NetherFortress(super::structures::nether_fortress::FortressPiece),
    Mineshaft(super::structures::mineshaft::MineshaftPiece),
    RuinedPortal(Box<super::structures::ruined_portal::RuinedPortalPiece>),
    JungleTemple(super::structures::jungle_temple::JungleTemplePiece),
    SwampHut(super::structures::swamp_hut::SwampHutPiece),
    DesertPyramid(Box<super::structures::desert_pyramid::DesertPyramidPiece>),
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
            Self::Stronghold(p) => p.data.bounds,
            Self::NetherFortress(p) => p.data.bounds,
            Self::Mineshaft(p) => p.data.bounds,
            Self::RuinedPortal(p) => p.template.data.bounds,
            Self::JungleTemple(p) => p.scattered.data.bounds,
            Self::SwampHut(p) => p.scattered.data.bounds,
            Self::DesertPyramid(p) => p.scattered.data.bounds,
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
            Self::Stronghold(p) => Ok(p.to_nbt()),
            Self::NetherFortress(p) => Ok(p.to_nbt()),
            Self::Mineshaft(p) => Ok(p.to_nbt()),
            Self::RuinedPortal(p) => Ok(p.to_nbt()),
            Self::JungleTemple(p) => Ok(p.to_nbt()),
            Self::SwampHut(p) => Ok(p.to_nbt()),
            Self::DesertPyramid(p) => Ok(p.to_nbt()),
        }
    }
}
/// Port of StructureStart. The bounding-box cache survives piece mutation, as
/// vanilla's cache does. The owning chunk exclusively borrows mutable starts.
#[derive(Clone, Debug)]
pub struct StructureStart {
    pub structure: Option<Identifier>,
    pub kind: Option<worldgen_structure_type::Id>,
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
            kind: None,
            chunk: [0; 2],
            references: 0,
            pieces: Vec::new(),
            terrain: TerrainAdjustment::None,
            cached_bounds: OnceLock::new(),
        }
    }
    pub fn new(
        structure: Identifier,
        kind: worldgen_structure_type::Id,
        chunk: [i32; 2],
        references: i32,
        terrain: TerrainAdjustment,
        pieces: Vec<StructurePiece>,
    ) -> Self {
        Self {
            structure: Some(structure),
            kind: Some(kind),
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
            worldgen_structure_type::Id::from_name("minecraft:jigsaw")
                .expect("builtin structure type"),
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
        settings: &super::pipeline::StructureRegistry,
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
        let id = id.to_string_lossy().parse::<Identifier>()?;
        let Some(definition) = settings.structures.get(&id) else {
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
                    "minecraft:shstart" | "minecraft:shsd" | "minecraft:shs" | "minecraft:shph"
                    | "minecraft:shlt" | "minecraft:shrt" | "minecraft:shrc"
                    | "minecraft:shssd" | "minecraft:sh5c" | "minecraft:shcc"
                    | "minecraft:shli" | "minecraft:shpr" | "minecraft:shfc" => {
                        if let Ok(piece) = super::structures::stronghold::StrongholdPiece::from_nbt(
                            &piece_id.to_string(),
                            tag,
                        ) {
                            pieces.push(StructurePiece::Stronghold(piece));
                        }
                    }
                    "minecraft:nestart" | "minecraft:nebcr" | "minecraft:nebs"
                    | "minecraft:nebef" | "minecraft:nerc" | "minecraft:nesr"
                    | "minecraft:nemt" | "minecraft:nece" | "minecraft:nesc"
                    | "minecraft:nescsc" | "minecraft:nescrt" | "minecraft:nesclt"
                    | "minecraft:neccs" | "minecraft:nectb" | "minecraft:necsr" => {
                        if let Ok(piece) =
                            super::structures::nether_fortress::FortressPiece::from_nbt(
                                &piece_id.to_string(),
                                tag,
                            )
                        {
                            pieces.push(StructurePiece::NetherFortress(piece));
                        }
                    }
                    "minecraft:msroom"
                    | "minecraft:mscorridor"
                    | "minecraft:msstairs"
                    | "minecraft:mscrossing" => {
                        if let Ok(piece) = super::structures::mineshaft::MineshaftPiece::from_nbt(
                            &piece_id.to_string(),
                            tag,
                        ) {
                            pieces.push(StructurePiece::Mineshaft(piece));
                        }
                    }
                    "minecraft:rupo" => {
                        if let Ok(piece) =
                            super::structures::ruined_portal::RuinedPortalPiece::from_nbt(
                                templates, tag,
                            )
                        {
                            pieces.push(StructurePiece::RuinedPortal(Box::new(piece)));
                        }
                    }
                    "minecraft:tejp" => {
                        if let Ok(piece) =
                            super::structures::jungle_temple::JungleTemplePiece::from_nbt(tag)
                        {
                            pieces.push(StructurePiece::JungleTemple(piece));
                        }
                    }
                    "minecraft:tedp" => {
                        if let Ok(piece) =
                            super::structures::desert_pyramid::DesertPyramidPiece::from_nbt(tag)
                        {
                            pieces.push(StructurePiece::DesertPyramid(Box::new(piece)));
                        }
                    }
                    "minecraft:tesh" => {
                        if let Ok(piece) =
                            super::structures::swamp_hut::SwampHutPiece::from_nbt(tag)
                        {
                            pieces.push(StructurePiece::SwampHut(piece));
                        }
                    }
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
            definition.kind,
            [
                tag.get_int("ChunkX").unwrap_or(0),
                tag.get_int("ChunkZ").unwrap_or(0),
            ],
            tag.get_int("references").unwrap_or(0),
            definition.terrain,
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
        let data = super::piece::StructurePieceData::from_nbt(tag)?;
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
            gen_depth: data.gen_depth,
            orientation: data.orientation,
            junctions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::{pipeline::StructureRegistry, processor::tests::generator};
    use serde_json::Value;
    use std::io::Read;
    #[test]
    fn start_codec_numeric_bounds_aliases_invalid_children_and_ids_match_vanilla() {
        let (resources, generator) = generator();
        let registry = StructureRegistry::load(&resources, &generator.data).unwrap();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("testdata/structure_start_codec.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let input = ferraloom_nbt::parse_compound(case["input"].as_str().unwrap()).unwrap();
            let result =
                StructureStart::from_nbt(&input, &registry, &templates, &resources, &mut |_| {
                    Err(Error("no inline features".into()))
                });
            match case["outcome"].as_str().unwrap() {
                "throws" => assert!(result.is_err(), "{input:?}"),
                "null" => assert!(result.unwrap().is_none(), "{input:?}"),
                "start" => assert_eq!(
                    result.unwrap().unwrap().to_nbt([-7, 11]).unwrap(),
                    ferraloom_nbt::parse_compound(case["output"].as_str().unwrap()).unwrap(),
                    "{input:?}"
                ),
                _ => unreachable!(),
            }
        }
    }
}
