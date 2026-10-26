use super::*;
use crate::{
    Behaviors,
    entity::{Cow, EntityData, EntityDimensions, EntityRef, EntityState},
    level::{
        Level, LevelContext, LevelError, Region, chunk::LevelChunk,
        path_navigation_region::PathNavigationRegion,
    },
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{
    Block, BlockState, EntityType, FluidState,
    datapack::{Pack, ResourceManager, TagSet},
    registries::fluid,
    tags::FluidTags,
};
use ferraloom_util::{
    Aabb, BlockPos, ChunkPos, Vec3,
    collections::HashSet,
    random::{LegacyRandomSource, RandomSource},
};
use serde_json::{Value, json};
use std::{
    io::Read,
    sync::{Arc, OnceLock},
};

static EMPTY: Behaviors = Behaviors::EMPTY;
struct Tags {
    blocks: Arc<TagSet<Block>>,
    fluids: Arc<TagSet<fluid::Id>>,
    entities: Arc<TagSet<EntityType>>,
}
fn level() -> Level {
    static TAGS: OnceLock<Tags> = OnceLock::new();
    let tags = TAGS.get_or_init(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder native resources");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        Tags {
            blocks: Arc::new(TagSet::load(&resources).unwrap()),
            fluids: Arc::new(TagSet::load(&resources).unwrap()),
            entities: Arc::new(TagSet::load(&resources).unwrap()),
        }
    });
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.install_shared(Box::default());
    level.set_block_tags(tags.blocks.clone());
    level.set_fluid_tags(tags.fluids.clone());
    level.set_entity_type_tags(tags.entities.clone());
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            -4..20,
            LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            ),
        ))
        .unwrap();
    level
}
fn pos(value: &Value) -> BlockPos {
    BlockPos::new(
        value[0].as_i64().unwrap() as i32,
        value[1].as_i64().unwrap() as i32,
        value[2].as_i64().unwrap() as i32,
    )
}
fn vec(value: &Value) -> Vec3 {
    Vec3::new(
        value[0].as_f64().unwrap(),
        value[1].as_f64().unwrap(),
        value[2].as_f64().unwrap(),
    )
}
fn bits(value: f32) -> i32 {
    value.to_bits() as i32
}
fn node(node: &Node) -> Value {
    json!({"pos":[node.pos.x,node.pos.y,node.pos.z],"type":node.path_type.id(),"g":bits(node.g),"h":bits(node.h),"f":bits(node.f),"malus":bits(node.cost_malus),"walked":bits(node.walked_distance),"closed":node.closed})
}
struct Probe {
    receiver: EntityRef,
    width: f32,
    height: f32,
    ground: bool,
    water: bool,
    floatable: bool,
    stand_water: bool,
    stand_lava: bool,
    step: f32,
    fall: i32,
    maluses: [Option<f32>; 27],
    starts: i32,
    ends: i32,
    writes: Vec<String>,
    random: LegacyRandomSource,
}
impl MobAccess for Probe {
    fn position(&self, level: &Level) -> Result<Vec3, LevelError> {
        Ok(level.entity::<EntityData>(self.receiver)?.position)
    }
    fn bounding_box(&self, level: &Level) -> Result<Aabb, LevelError> {
        Ok(level.entity::<EntityData>(self.receiver)?.bounding_box)
    }
    fn dimensions(&self, _level: &Level) -> Result<(f32, f32), LevelError> {
        Ok((self.width, self.height))
    }
    fn on_ground(&self, _level: &Level) -> Result<bool, LevelError> {
        Ok(self.ground)
    }
    fn is_in_water(&self, _level: &Level) -> Result<bool, LevelError> {
        Ok(self.water)
    }
    fn is_in_floatable_fluid(&self, _level: &Level) -> Result<bool, LevelError> {
        Ok(self.floatable)
    }
    fn can_stand_on_fluid(&self, fluid: FluidState, level: &Level) -> Result<bool, LevelError> {
        Ok(self.stand_water
            && level
                .fluid_tags()?
                .contains_key(FluidTags::WATER, fluid.fluid_type)
            || self.stand_lava
                && level
                    .fluid_tags()?
                    .contains_key(FluidTags::LAVA, fluid.fluid_type))
    }
    fn max_up_step(&self, _level: &Level) -> Result<f32, LevelError> {
        Ok(self.step)
    }
    fn max_fall_distance(&self, _level: &Level) -> Result<i32, LevelError> {
        Ok(self.fall)
    }
    fn pathfinding_malus(&self, path_type: PathType, _level: &Level) -> Result<f32, LevelError> {
        Ok(self.maluses[path_type.id() as usize].unwrap_or(path_type.malus()))
    }
    fn set_pathfinding_malus(
        &mut self,
        path_type: PathType,
        value: f32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        self.maluses[path_type.id() as usize] = Some(value);
        self.writes
            .push(format!("{}:{}", path_type.id(), bits(value)));
        Ok(())
    }
    fn on_pathfinding_start(&mut self, _level: &mut Level) -> Result<(), LevelError> {
        self.starts += 1;
        Ok(())
    }
    fn on_pathfinding_done(&mut self, _level: &mut Level) -> Result<(), LevelError> {
        self.ends += 1;
        Ok(())
    }
    fn next_random_int(&mut self, bound: i32, _level: &mut Level) -> Result<i32, LevelError> {
        Ok(self.random.next_int_bound(bound))
    }
    fn collision_source(&self) -> EntityRef {
        self.receiver
    }
}
fn probe(row: &Value, level: &mut Level) -> Probe {
    let width = row["width"].as_f64().unwrap() as f32;
    let height = row["height"].as_f64().unwrap() as f32;
    let receiver = level.next_entity_ref();
    let mut data = EntityData::new(EntityType::COW, level.next_entity_id().unwrap(), 0);
    data.dimensions = EntityDimensions::scalable(width, height);
    data.set_pos(vec(&row["position"]));
    level
        .add_entity(receiver, EntityState::Cow(Box::new(Cow::with_parent(data))))
        .unwrap();
    Probe {
        receiver,
        width,
        height,
        ground: row["flags"].as_i64().unwrap() & 16 == 0,
        water: row["water"].as_bool().unwrap(),
        floatable: row["floatable"].as_bool().unwrap(),
        stand_water: row["stand_water"].as_bool().unwrap(),
        stand_lava: row["stand_lava"].as_bool().unwrap(),
        step: row["step"].as_f64().unwrap() as f32,
        fall: row["fall"].as_i64().unwrap() as i32,
        maluses: [None; 27],
        starts: 0,
        ends: 0,
        writes: Vec::new(),
        random: LegacyRandomSource::new(12345),
    }
}
trait PreparedEvaluator: NodeEvaluator<Error = LevelError> {
    fn classify(&mut self, pos: BlockPos) -> Result<PathType, LevelError>;
}
macro_rules! classify {
    ($name:ident) => {
        impl PreparedEvaluator for $name<'_, Probe> {
            fn classify(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
                self.get_path_type_of_mob(pos)
            }
        }
    };
}
classify!(WalkNodeEvaluator);
classify!(FlyNodeEvaluator);
classify!(AmphibiousNodeEvaluator);
impl PreparedEvaluator for SwimNodeEvaluator<'_, Probe> {
    fn classify(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        self.get_path_type(pos)
    }
}
fn run<E: PreparedEvaluator>(evaluator: &mut E, row: &Value) -> Result<Value, String> {
    let mut data = PathData::default();
    if row["op"] == "neighbors" {
        evaluator
            .prepare(&mut data)
            .map_err(|e| format!("prepare {e:?}"))?;
        let start = evaluator
            .get_start(&mut data)
            .map_err(|e| format!("start {e:?}"))?
            .unwrap();
        let start_state = node(&data[start]);
        let mut out = Vec::new();
        evaluator
            .get_neighbors(start, &mut out, &mut data)
            .map_err(|e| format!("neighbors {e:?}"))?;
        let nodes = out
            .into_iter()
            .map(|id| node(&data[id]))
            .collect::<Vec<_>>();
        let mut types = Vec::new();
        for x in 4..=8 {
            for y in 0..=3 {
                for z in 7..=9 {
                    let p = BlockPos::new(x, y, z);
                    types.push(json!({"pos":[x,y,z],"type":evaluator.classify(p).map_err(|e|format!("type {e:?}"))?.id()}));
                }
            }
        }
        evaluator
            .done(&mut data)
            .map_err(|e| format!("done {e:?}"))?;
        Ok(json!({"start":start_state,"nodes":nodes,"types":types}))
    } else {
        let targets = row["targets"]
            .as_array()
            .unwrap()
            .iter()
            .map(pos)
            .collect::<HashSet<_>>();
        let mut finder = PathFinder::new(row["budget"].as_i64().unwrap() as i32);
        let path = finder
            .find_path(
                evaluator,
                &mut data,
                &targets,
                32.0,
                row["reach"].as_i64().unwrap() as i32,
                1.0,
            )
            .map_err(|e| format!("search {e:?}"))?;
        Ok(match path {
            None => json!({"nodes":null}),
            Some(path) => {
                let target = path.target();
                json!({"nodes":path.nodes(&data).iter().map(|&id|node(&data[id])).collect::<Vec<_>>(),"reached":path.can_reach(),"dist":bits(path.dist_to_target()),"target":[target.x,target.y,target.z]})
            }
        })
    }
}
fn check(kind: &str) {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/entity/pathfinder/evaluators.json.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let rows: Vec<Value> = serde_json::from_str(&text).unwrap();
    let mut failures = Vec::new();
    let mut count = 0;
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row["kind"] == kind)
    {
        count += 1;
        let mut level = level();
        for block in row["blocks"].as_array().unwrap() {
            level
                .set_block_state(
                    pos(&block["pos"]),
                    BlockState::from_id(block["state"].as_u64().unwrap() as u16).unwrap(),
                )
                .unwrap();
        }
        let region = PathNavigationRegion::capture(
            &level,
            BlockPos::new(0, -64, 0),
            BlockPos::new(15, 319, 15),
        )
        .unwrap();
        let mut mob = probe(row, &mut level);
        let flags = row["flags"].as_i64().unwrap();
        let capabilities = Capabilities {
            can_float: flags & 1 != 0,
            can_open_doors: flags & 2 != 0,
            can_pass_doors: flags & 4 == 0,
            can_walk_over_fences: flags & 8 != 0,
        };
        let actual = match kind {
            "walk" => {
                let mut e = WalkNodeEvaluator::new(&region, &mut level, &mut mob);
                *e.capabilities_mut() = capabilities;
                run(&mut e, row)
            }
            "fly" => {
                let mut e = FlyNodeEvaluator::new(&region, &mut level, &mut mob);
                *e.capabilities_mut() = capabilities;
                run(&mut e, row)
            }
            "swim" => {
                let mut e = SwimNodeEvaluator::new(&region, &mut level, &mut mob, flags & 8 != 0);
                *e.capabilities_mut() = capabilities;
                run(&mut e, row)
            }
            "amphibious" => {
                let mut e =
                    AmphibiousNodeEvaluator::new(&region, &mut level, &mut mob, flags & 8 != 0);
                *e.capabilities_mut() = capabilities;
                run(&mut e, row)
            }
            _ => unreachable!(),
        };
        let mut expected = json!({"nodes":row["nodes"]});
        for key in ["start", "target", "reached", "dist", "types"] {
            if let Some(value) = row.get(key) {
                expected[key] = value.clone();
            }
        }
        let result = actual.and_then(|value| {
            if value == expected {
                Ok(())
            } else {
                Err(format!("expected {expected}, actual {value}"))
            }
        });
        let rng = mob.random.next_int();
        let lifecycle =
            json!({"starts":mob.starts,"ends":mob.ends,"writes":mob.writes,"next_random":rng});
        let expected_lifecycle = json!({"starts":row["starts"],"ends":row["ends"],"writes":row["writes"],"next_random":row["next_random"]});
        if let Err(error) = result {
            failures.push(format!(
                "case {index} {} width {} flags {} {}: {error}",
                row["layout"], row["width"], flags, row["op"]
            ));
        }
        if lifecycle != expected_lifecycle {
            failures.push(format!(
                "case {index} lifecycle expected {expected_lifecycle}, actual {lifecycle}"
            ));
        }
    }
    assert!(count > 400);
    assert!(
        failures.is_empty(),
        "{} {kind} failures:\n{}",
        failures.len(),
        failures
            .iter()
            .take(16)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("{count} native {kind} evaluator/search cases PASS");
}
#[test]
fn native_walk_paths_neighbors_costs_and_rng() {
    check("walk");
}
#[test]
fn native_fly_paths_neighbors_costs_and_rng() {
    check("fly");
}
#[test]
fn native_swim_paths_neighbors_costs_and_rng() {
    check("swim");
}
#[test]
fn native_amphibious_paths_neighbors_costs_and_rng() {
    check("amphibious");
}
