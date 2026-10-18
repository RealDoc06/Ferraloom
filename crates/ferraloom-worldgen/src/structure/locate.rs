//! ChunkGenerator's map-structure search, with synchronous chunk/status access
//! supplied by the owning level. Search state and ordering are local to one call.
use super::{check::StructureCheckResult, placement::StructurePlacement};
use crate::{Error, Identifier};

/// A placement's structures have already been filtered against the generator's
/// possible biomes. Entries follow possibleStructureSets registry order. Reusing
/// the same placement reference preserves vanilla's object-identity grouping.
pub struct LocatePlacement<'a> {
    pub placement: &'a StructurePlacement,
    pub structures: &'a [Identifier],
    pub rings: Option<&'a [[i32; 2]]>,
}
#[derive(Clone, Copy, Debug)]
pub struct LocateRequest {
    pub position: [i32; 3],
    pub max_search_radius: i32,
    pub create_reference: bool,
    /// Both world generateStructures and the debug feature gate must allow work.
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedStructure {
    pub position: [i32; 3],
    pub structure: Identifier,
}
/// Information read from the actual start after a STRUCTURE_STARTS chunk load.
/// The owning level keeps the canonical start, never a shared mutable copy.
#[derive(Clone, Copy, Debug)]
pub struct LocateStart {
    pub chunk: [i32; 2],
    pub valid: bool,
    pub references: i32,
}
/// Native StructureManager and LevelReader operations used by /locate and maps.
/// Calls are synchronous under the level's exclusive ownership. add_reference
/// updates both StructureStart and StructureCheck, in that order.
pub trait StructureLocateAccess {
    fn check_structure_presence(
        &mut self,
        chunk: [i32; 2],
        structure: &Identifier,
        placement: &StructurePlacement,
        require_unreferenced: bool,
    ) -> Result<StructureCheckResult, Error>;
    fn load_start(
        &mut self,
        chunk: [i32; 2],
        structure: &Identifier,
    ) -> Result<Option<LocateStart>, Error>;
    fn add_reference(
        &mut self,
        loaded_chunk: [i32; 2],
        origin: [i32; 2],
        structure: &Identifier,
    ) -> Result<(), Error>;
}
pub struct StructureLocator<'a> {
    pub seed: i64,
    pub placements: Vec<LocatePlacement<'a>>,
}
fn distance(a: [i32; 3], b: [i32; 3]) -> f64 {
    let [x, y, z] = std::array::from_fn(|i| f64::from(a[i]) - f64::from(b[i]));
    x * x + y * y + z * z
}
impl StructureLocator<'_> {
    /// Port of ChunkGenerator#findNearestMapStructure. Ring search uses chunk
    /// centers at Y=32 for pruning, while final comparisons use locate offsets.
    /// Random spread stops at the first successful perimeter, with X/Z traversal
    /// and placement order preserved even when a later hit is closer.
    pub fn find_nearest(
        &self,
        wanted: &[Identifier],
        request: LocateRequest,
        access: &mut impl StructureLocateAccess,
    ) -> Result<Option<LocatedStructure>, Error> {
        if !request.enabled {
            return Ok(None);
        }
        let mut scans: Vec<(usize, Vec<&Identifier>)> = Vec::new();
        for structure in wanted {
            for (index, entry) in self.placements.iter().enumerate() {
                if !entry.structures.contains(structure) {
                    continue;
                }
                let group = if let Some(i) = scans
                    .iter()
                    .position(|(i, _)| std::ptr::eq(self.placements[*i].placement, entry.placement))
                {
                    i
                } else {
                    scans.push((index, Vec::new()));
                    scans.len() - 1
                };
                if !scans[group].1.contains(&structure) {
                    scans[group].1.push(structure);
                }
            }
        }
        let mut nearest = None;
        let mut nearest_distance = f64::MAX;
        let mut spreads = Vec::new();
        for (index, structures) in &scans {
            let entry = &self.placements[*index];
            match entry.placement {
                StructurePlacement::ConcentricRings(_) => {
                    let rings = entry.rings.ok_or_else(|| {
                        Error("missing positions for concentric-ring placement".into())
                    })?;
                    let mut closest = None;
                    let mut closest_distance = f64::MAX;
                    for &chunk in rings {
                        let center = [
                            chunk[0].wrapping_mul(16).wrapping_add(8),
                            32,
                            chunk[1].wrapping_mul(16).wrapping_add(8),
                        ];
                        let approximate = distance(center, request.position);
                        if (closest.is_none() || approximate < closest_distance)
                            && let Some(found) = generating_at(
                                structures,
                                entry.placement,
                                chunk,
                                request.create_reference,
                                access,
                            )?
                        {
                            closest = Some(found);
                            closest_distance = approximate;
                        }
                    }
                    if let Some(found) = closest {
                        let d = distance(request.position, found.position);
                        if d < nearest_distance {
                            nearest_distance = d;
                            nearest = Some(found);
                        }
                    }
                }
                StructurePlacement::RandomSpread(_) => spreads.push((*index, structures)),
                StructurePlacement::DimensionOrigin => {}
            }
        }
        if spreads.is_empty() {
            return Ok(nearest);
        }
        let origin = [request.position[0] >> 4, request.position[2] >> 4];
        for radius in 0..=request.max_search_radius {
            let mut any = false;
            for &(index, structures) in &spreads {
                let entry = &self.placements[index];
                let StructurePlacement::RandomSpread(config) = entry.placement else {
                    unreachable!("spread scan");
                };
                let mut hit = None;
                'perimeter: for dx in -radius..=radius {
                    let edge_x = dx == -radius || dx == radius;
                    for dz in -radius..=radius {
                        if !edge_x && dz != -radius && dz != radius {
                            continue;
                        }
                        let sector = [
                            origin[0].wrapping_add(config.spacing.wrapping_mul(dx)),
                            origin[1].wrapping_add(config.spacing.wrapping_mul(dz)),
                        ];
                        let chunk = config.potential_structure_chunk(self.seed, sector);
                        if let Some(found) = generating_at(
                            structures,
                            entry.placement,
                            chunk,
                            request.create_reference,
                            access,
                        )? {
                            hit = Some(found);
                            break 'perimeter;
                        }
                    }
                }
                if let Some(found) = hit {
                    any = true;
                    let d = distance(request.position, found.position);
                    if d < nearest_distance {
                        nearest_distance = d;
                        nearest = Some(found);
                    }
                }
            }
            if any {
                return Ok(nearest);
            }
        }
        Ok(nearest)
    }
}
/// Port of getStructureGeneratingAt. Cheap known-present checks avoid loading
/// chunks only when no reference is requested; uncertain checks load starts.
fn generating_at(
    structures: &[&Identifier],
    placement: &StructurePlacement,
    target: [i32; 2],
    create_reference: bool,
    access: &mut impl StructureLocateAccess,
) -> Result<Option<LocatedStructure>, Error> {
    for &structure in structures {
        let presence =
            access.check_structure_presence(target, structure, placement, create_reference)?;
        if presence == StructureCheckResult::StartNotPresent {
            continue;
        }
        if !create_reference && presence == StructureCheckResult::StartPresent {
            return Ok(Some(LocatedStructure {
                position: placement.locate_pos(target),
                structure: structure.clone(),
            }));
        }
        if let Some(start) = access.load_start(target, structure)?
            && start.valid
            && (!create_reference || start.references < 1)
        {
            if create_reference {
                access.add_reference(target, start.chunk, structure)?;
            }
            return Ok(Some(LocatedStructure {
                position: placement.locate_pos(start.chunk),
                structure: structure.clone(),
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{collections::BTreeMap, io::Read};
    struct Access<'a> {
        ids: &'a [Identifier],
        placements: &'a [StructurePlacement],
        mode: i32,
        trace: Vec<Value>,
        loaded: BTreeMap<(usize, [i32; 2]), LocateStart>,
    }
    impl Access<'_> {
        fn index(&self, id: &Identifier) -> usize {
            self.ids.iter().position(|i| i == id).unwrap()
        }
    }
    impl StructureLocateAccess for Access<'_> {
        fn check_structure_presence(
            &mut self,
            chunk: [i32; 2],
            structure: &Identifier,
            placement: &StructurePlacement,
            require: bool,
        ) -> Result<StructureCheckResult, Error> {
            let id = self.index(structure);
            let h = chunk[0]
                .wrapping_mul(31)
                .wrapping_add(chunk[1].wrapping_mul(17))
                .wrapping_add((id as i32) * 13)
                .wrapping_add(self.mode * 7)
                .rem_euclid(11);
            let result = if self.mode == 0 {
                1
            } else if self.mode == 1 {
                if h < 8 { 0 } else { 2 }
            } else if h < 4 {
                1
            } else if h < 7 {
                0
            } else {
                2
            };
            let index = self
                .placements
                .iter()
                .position(|p| std::ptr::eq(p, placement))
                .unwrap();
            self.trace.push(json!([
                "check", id, chunk[0], chunk[1], index, require, result
            ]));
            Ok(match result {
                0 => StructureCheckResult::StartPresent,
                1 => StructureCheckResult::StartNotPresent,
                _ => StructureCheckResult::ChunkLoadNeeded,
            })
        }
        fn load_start(
            &mut self,
            chunk: [i32; 2],
            structure: &Identifier,
        ) -> Result<Option<LocateStart>, Error> {
            let id = self.index(structure);
            self.trace.push(json!(["chunk", chunk[0], chunk[1]]));
            self.trace.push(json!(["load", id, chunk[0], chunk[1]]));
            if let Some(loaded) = self.loaded.get(&(id, chunk)) {
                return Ok(Some(*loaded));
            }
            let h = chunk[0]
                .wrapping_mul(7)
                .wrapping_add(chunk[1].wrapping_mul(3))
                .wrapping_add(id as i32)
                .wrapping_add(self.mode)
                .rem_euclid(7);
            if h == 0 {
                return Ok(None);
            }
            if h == 1 {
                return Ok(Some(LocateStart {
                    chunk: [0, 0],
                    valid: false,
                    references: 0,
                }));
            }
            let references = match h {
                2 => 1,
                3 => 2,
                4 => -1,
                _ => 0,
            };
            let origin = [
                chunk[0].wrapping_add(i32::from(h == 5)),
                chunk[1].wrapping_sub(i32::from(h == 6)),
            ];
            let start = LocateStart {
                chunk: origin,
                valid: true,
                references,
            };
            self.loaded.insert((id, chunk), start);
            Ok(Some(start))
        }
        fn add_reference(
            &mut self,
            loaded_chunk: [i32; 2],
            origin: [i32; 2],
            structure: &Identifier,
        ) -> Result<(), Error> {
            let id = self.index(structure);
            let start = self.loaded.get_mut(&(id, loaded_chunk)).unwrap();
            assert_eq!(start.chunk, origin);
            start.references = start.references.wrapping_add(1);
            self.trace.push(json!([
                "reference",
                id,
                origin[0],
                origin[1],
                start.references
            ]));
            Ok(())
        }
    }
    #[test]
    fn locate_ring_pruning_spread_perimeters_offsets_loads_references_and_ties_match_vanilla() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("testdata/locate.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let fixture: Value = serde_json::from_str(&text).unwrap();
        let placements = fixture["placements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| StructurePlacement::decode(v).unwrap())
            .collect::<Vec<_>>();
        let ids = [
            "stronghold",
            "village_plains",
            "mineshaft",
            "pillager_outpost",
            "igloo",
        ]
        .map(|s| format!("minecraft:{s}").parse::<Identifier>().unwrap());
        let members = [
            vec![ids[1].clone(), ids[2].clone()],
            vec![ids[2].clone()],
            vec![ids[0].clone(), ids[1].clone()],
            vec![ids[3].clone()],
        ];
        let rings = fixture["rings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| std::array::from_fn(|i| p[i].as_i64().unwrap() as i32))
            .collect::<Vec<[i32; 2]>>();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let mode = case["mode"].as_i64().unwrap() as i32;
            let seed = case["seed"].as_i64().unwrap();
            let entries = placements
                .iter()
                .enumerate()
                .map(|(i, p)| LocatePlacement {
                    placement: p,
                    structures: &members[i],
                    rings: if i == 2 && mode != 3 {
                        Some(rings.as_slice())
                    } else {
                        None
                    },
                })
                .collect();
            let locator = StructureLocator {
                seed,
                placements: entries,
            };
            let wanted = case["wanted"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| ids[i.as_u64().unwrap() as usize].clone())
                .collect::<Vec<_>>();
            let request = LocateRequest {
                position: std::array::from_fn(|i| case["position"][i].as_i64().unwrap() as i32),
                max_search_radius: case["radius"].as_i64().unwrap() as i32,
                create_reference: case["reference"].as_bool().unwrap(),
                enabled: case["enabled"].as_bool().unwrap(),
            };
            let mut access = Access {
                ids: &ids,
                placements: &placements,
                mode,
                trace: Vec::new(),
                loaded: BTreeMap::new(),
            };
            let result = locator.find_nearest(&wanted, request, &mut access);
            let label = format!("case={index} seed={seed} mode={mode} request={request:?}");
            match case["outcome"].as_str().unwrap() {
                "throws" => assert!(result.is_err(), "{label}"),
                "none" => assert_eq!(result.unwrap(), None, "{label}"),
                "found" => {
                    let p = &case["result"];
                    assert_eq!(
                        result.unwrap(),
                        Some(LocatedStructure {
                            position: std::array::from_fn(|i| p[i].as_i64().unwrap() as i32),
                            structure: ids[p[3].as_u64().unwrap() as usize].clone()
                        }),
                        "{label}"
                    );
                }
                other => panic!("unknown outcome {other}"),
            }
            let expected = case["trace"].as_array().unwrap();
            assert_eq!(access.trace.len(), expected.len(), "{label} operations");
            for (i, (a, b)) in access.trace.iter().zip(expected).enumerate() {
                assert_eq!(a, b, "{label} operation={i}");
            }
        }
    }
}
