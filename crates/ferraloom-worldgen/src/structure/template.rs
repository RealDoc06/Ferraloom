//! StructureTemplate's resource loading, palette order and coordinate geometry.
mod manager;
mod placement;
use super::BoundingBox;
use crate::{Error, Identifier};
use ferraloom_data::{
    Block, BlockState,
    datapack::{ResourceManager, decode_block_state},
};
use ferraloom_nbt::{CompoundTag, Compression, NbtAccounter, NbtIo, Tag};
use ferraloom_util::{
    Mirror, Rotation, mth,
    random::{LegacyRandomSource, Random, RandomSource},
};
pub use manager::StructureTemplateManager;
use std::{collections::BTreeMap, sync::Arc};

/// Port of StructureTemplate.StructureBlockInfo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructureBlockInfo {
    pub position: [i32; 3],
    pub state: BlockState,
    pub nbt: Option<CompoundTag>,
}
/// Port of StructureTemplate.StructureEntityInfo.
#[derive(Clone, Debug)]
pub struct StructureEntityInfo {
    pub position: [f64; 3],
    pub block_position: [i32; 3],
    pub nbt: CompoundTag,
}
/// Port of StructureTemplate.Palette. Blocks are ordered full, other, entities;
/// each partition is stably sorted Y/X/Z, never registry or hash-map order.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    pub blocks: Vec<StructureBlockInfo>,
}
#[derive(Clone, Debug, Default)]
pub struct StructureTemplate {
    pub size: [i32; 3],
    pub palettes: Vec<Palette>,
    pub entities: Vec<StructureEntityInfo>,
}

fn list<'a>(tag: &'a CompoundTag, key: &str) -> &'a [Tag] {
    tag.get(key)
        .and_then(Tag::as_list)
        .map_or(&[], |v| v.as_slice())
}
fn compound<'a>(tag: &'a CompoundTag, key: &str) -> Option<&'a CompoundTag> {
    tag.get(key).and_then(Tag::as_compound)
}
fn block_position(v: &[Tag]) -> [i32; 3] {
    std::array::from_fn(|i| v.get(i).and_then(Tag::as_int).unwrap_or(0))
}
/// NbtUtils#readBlockState has an AIR fallback, unlike BlockState.CODEC's error
/// on an unknown block. Property names and invalid values remain lenient.
pub fn read_block_state(tag: &CompoundTag) -> BlockState {
    let Some(name) = tag.get_string("id") else {
        return Block::AIR.default_state();
    };
    let mut value = serde_json::json!({"id":name.to_string_lossy()});
    if let Some(properties) = compound(tag, "properties") {
        let values = properties
            .iter()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy(),
                    serde_json::Value::String(v.as_string()?.to_string_lossy()),
                ))
            })
            .collect::<serde_json::Map<_, _>>();
        value["properties"] = serde_json::Value::Object(values);
    }
    decode_block_state(&value).unwrap_or_else(|_| Block::AIR.default_state())
}
impl StructureTemplate {
    /// Port of StructureTemplate#load on canonical, current-version NBT. DFU
    /// conversion belongs before this function, as in TemplateSource.
    pub fn from_nbt(tag: &CompoundTag) -> Self {
        let mut result = Self {
            size: block_position(list(tag, "size")),
            ..Self::default()
        };
        let blocks = list(tag, "blocks");
        let empty = CompoundTag::new();
        let mut load_palette = |values: &[Tag]| {
            let states = values
                .iter()
                .map(|v| read_block_state(v.as_compound().unwrap_or(&empty)))
                .collect::<Vec<_>>();
            let mut entries = blocks
                .iter()
                .filter_map(Tag::as_compound)
                .map(|v| {
                    let index = v.get_int("state").unwrap_or(0);
                    let state = usize::try_from(index)
                        .ok()
                        .and_then(|i| states.get(i))
                        .copied()
                        .unwrap_or_else(|| Block::AIR.default_state());
                    StructureBlockInfo {
                        position: block_position(list(v, "pos")),
                        state,
                        nbt: compound(v, "nbt").cloned(),
                    }
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|b| {
                let state = b.state.data();
                let full = !state.context_dependent_shape
                    && state
                        .collision
                        .boxes()
                        .iter()
                        .any(|b| b.min == [0.0; 3] && b.max == [1.0; 3]);
                (
                    if b.nbt.is_some() {
                        2
                    } else if full {
                        0
                    } else {
                        1
                    },
                    b.position[1],
                    b.position[0],
                    b.position[2],
                )
            });
            result.palettes.push(Palette { blocks: entries });
        };
        if let Some(palettes) = tag.get("palettes").and_then(Tag::as_list) {
            for palette in palettes.iter() {
                load_palette(palette.as_list().map_or(&[], |v| v.as_slice()));
            }
        } else {
            load_palette(list(tag, "palette"));
        }
        for entity in list(tag, "entities").iter().filter_map(Tag::as_compound) {
            if let Some(nbt) = compound(entity, "nbt") {
                let pos = list(entity, "pos");
                result.entities.push(StructureEntityInfo {
                    position: std::array::from_fn(|i| {
                        pos.get(i).and_then(Tag::as_double).unwrap_or(0.0)
                    }),
                    block_position: block_position(list(entity, "blockPos")),
                    nbt: nbt.clone(),
                });
            }
        }
        result
    }
    pub fn load(resources: &ResourceManager, id: &Identifier) -> Result<Self, Error> {
        let resource = resources.require(&id.with_path(format!("structure/{}.nbt", id.path()))?)?;
        Self::load_bytes(resource.bytes, id)
    }
    fn load_bytes(bytes: &[u8], id: &Identifier) -> Result<Self, Error> {
        let (_, tag) = NbtIo::read(
            bytes,
            Compression::Gzip,
            true,
            &mut NbtAccounter::unlimited(),
        )
        .map_err(|e| Error(e.to_string()))?;
        let tag = tag
            .as_compound()
            .ok_or_else(|| Error(format!("structure {id} root is not a compound")))?;
        let version = tag.get_int("DataVersion").unwrap_or(500);
        if version != 5023 {
            return Err(Error(format!(
                "structure {id} has data version {version}; conversion to 5023 is required"
            )));
        }
        Ok(Self::from_nbt(tag))
    }
    pub fn rotated_size(&self, rotation: Rotation) -> [i32; 3] {
        match rotation {
            Rotation::Clockwise90 | Rotation::CounterClockwise90 => {
                [self.size[2], self.size[1], self.size[0]]
            }
            _ => self.size,
        }
    }
    pub fn bounding_box(
        &self,
        settings: &StructurePlaceSettings,
        position: [i32; 3],
    ) -> BoundingBox {
        BoundingBox::new(
            transform_block([0; 3], settings.mirror, settings.rotation, settings.pivot),
            transform_block(
                self.size.map(|v| v.wrapping_sub(1)),
                settings.mirror,
                settings.rotation,
                settings.pivot,
            ),
        )
        .moved(position)
    }
    /// Port of StructureTemplate#getZeroPositionWithTransform.
    pub fn zero_position_with_transform(
        &self,
        zero: [i32; 3],
        mirror: Mirror,
        rotation: Rotation,
    ) -> [i32; 3] {
        let sx = self.size[0].wrapping_sub(1);
        let sz = self.size[2].wrapping_sub(1);
        let mx = if mirror == Mirror::FrontBack { sx } else { 0 };
        let mz = if mirror == Mirror::LeftRight { sz } else { 0 };
        let delta = match rotation {
            Rotation::None => [mx, 0, mz],
            Rotation::Clockwise90 => [sz.wrapping_sub(mz), 0, mx],
            Rotation::Clockwise180 => [sx.wrapping_sub(mx), 0, sz.wrapping_sub(mz)],
            Rotation::CounterClockwise90 => [mz, 0, sx.wrapping_sub(mx)],
        };
        std::array::from_fn(|i| zero[i].wrapping_add(delta[i]))
    }
    pub fn random_palette<'a>(
        &'a self,
        settings: &mut StructurePlaceSettings,
        position: [i32; 3],
        shared: &mut impl RandomSource,
    ) -> Result<&'a Palette, Error> {
        if self.palettes.is_empty() {
            return Err(Error("No palettes".into()));
        }
        let index = settings.with_random(position, shared, |r| {
            r.next_int_bound(self.palettes.len() as i32)
        }) as usize;
        Ok(&self.palettes[index])
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LiquidSettings {
    #[default]
    ApplyWaterlogging,
    IgnoreWaterlogging,
}
/// Port of StructurePlaceSettings. Processors are supplied to processing as a
/// borrowed list; settings and its RNG policy belong to one placement job.
#[derive(Clone, Debug, Default)]
pub struct StructurePlaceSettings {
    pub mirror: Mirror,
    pub rotation: Rotation,
    pub pivot: [i32; 3],
    pub ignore_entities: bool,
    pub bounding_box: Option<BoundingBox>,
    pub liquid_settings: LiquidSettings,
    pub random: PlacementRandom,
    pub known_shape: bool,
    pub finalize_entities: bool,
}
/// RandomSource selection for StructurePlaceSettings#getRandom. Shared uses
/// the caller's placement RNG for palette, processors and block-entity loot.
#[derive(Clone, Debug, Default)]
pub enum PlacementRandom {
    #[default]
    Positional,
    Fixed(Random),
    Shared,
}
impl StructurePlaceSettings {
    pub fn with_random<T>(
        &mut self,
        position: [i32; 3],
        shared: &mut impl RandomSource,
        f: impl FnOnce(&mut dyn RandomSource) -> T,
    ) -> T {
        match &mut self.random {
            PlacementRandom::Fixed(random) => f(random),
            PlacementRandom::Shared => f(shared),
            PlacementRandom::Positional => f(&mut LegacyRandomSource::new(mth::get_seed(
                position[0],
                position[1],
                position[2],
            ))),
        }
    }
}
/// Port of StructureTemplate#transform(BlockPos, Mirror, Rotation, BlockPos).
pub fn transform_block(
    [mut x, y, mut z]: [i32; 3],
    mirror: Mirror,
    rotation: Rotation,
    [px, _, pz]: [i32; 3],
) -> [i32; 3] {
    match mirror {
        Mirror::LeftRight => z = z.wrapping_neg(),
        Mirror::FrontBack => x = x.wrapping_neg(),
        Mirror::None => {}
    }
    match rotation {
        Rotation::None => [x, y, z],
        Rotation::Clockwise90 => [
            px.wrapping_add(pz).wrapping_sub(z),
            y,
            pz.wrapping_sub(px).wrapping_add(x),
        ],
        Rotation::Clockwise180 => [
            px.wrapping_add(px).wrapping_sub(x),
            y,
            pz.wrapping_add(pz).wrapping_sub(z),
        ],
        Rotation::CounterClockwise90 => [
            px.wrapping_sub(pz).wrapping_add(z),
            y,
            px.wrapping_add(pz).wrapping_sub(x),
        ],
    }
}
/// Port of StructureTemplate#transform(Vec3,...). Entity mirrors include a
/// one-block offset that the integer block transform deliberately does not.
pub fn transform_entity(
    [mut x, y, mut z]: [f64; 3],
    mirror: Mirror,
    rotation: Rotation,
    [px, _, pz]: [i32; 3],
) -> [f64; 3] {
    match mirror {
        Mirror::LeftRight => z = 1.0 - z,
        Mirror::FrontBack => x = 1.0 - x,
        Mirror::None => {}
    }
    match rotation {
        Rotation::None => [x, y, z],
        Rotation::Clockwise90 => [
            f64::from(px.wrapping_add(pz).wrapping_add(1)) - z,
            y,
            f64::from(pz.wrapping_sub(px)) + x,
        ],
        Rotation::Clockwise180 => [
            f64::from(px.wrapping_add(px).wrapping_add(1)) - x,
            y,
            f64::from(pz.wrapping_add(pz).wrapping_add(1)) - z,
        ],
        Rotation::CounterClockwise90 => [
            f64::from(px.wrapping_sub(pz)) + z,
            y,
            f64::from(px.wrapping_add(pz).wrapping_add(1)) - x,
        ],
    }
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;
