use ferraloom_data::{
    EntityType,
    datapack::{Pack, ResourceManager},
};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use ferraloom_worldgen::{
    Error,
    generator::{GenerationData, WorldPreset},
    region::Heightmap,
    settings::DimensionType,
    spawn::{PlayerSpawnFinder, SpawnWorld, fixup_spawn_height},
};
use serde_json::Value;
use std::{io::Read, path::Path, sync::Arc};

fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn every_preset_spawn_origin_matches_real_vanilla_sampling() {
    let resources = ResourceManager::new(vec![
        Pack::vanilla_jar(Path::new(
            &std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap(),
        ))
        .unwrap(),
    ]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    for case in golden(include_bytes!("golden/spawn_origins.json.gz"))
        .as_array()
        .unwrap()
    {
        let name = case["preset"].as_str().unwrap().parse().unwrap();
        let seed = case["seed"].as_i64().unwrap();
        let preset = WorldPreset::load(&name, &resources, data.clone(), seed).unwrap();
        let origin = preset
            .dimensions
            .overworld()
            .generator
            .access()
            .get_origin()
            .unwrap();
        assert_eq!(
            serde_json::to_value(origin).unwrap(),
            case["origin"],
            "{name} seed={seed}"
        );
    }
}
struct CollisionWorld {
    floor: i32,
    ceiling: i32,
    calls: Vec<[f64; 6]>,
}
impl SpawnWorld for CollisionWorld {
    fn dimension(&self) -> DimensionType {
        DimensionType {
            min_y: -64,
            height: 384,
            logical_height: 384,
            has_ceiling: false,
            has_skylight: true,
            coordinate_scale: 1.0,
        }
    }
    fn ensure_chunk(&mut self, _: [i32; 2]) -> Result<(), Error> {
        Err(Error("unexpected chunk load".into()))
    }
    fn height(&mut self, _: Heightmap, _: i32, _: i32) -> Result<i32, Error> {
        Err(Error("unexpected height query".into()))
    }
    fn has_fluid(&mut self, _: [i32; 3]) -> Result<bool, Error> {
        Err(Error("unexpected fluid query".into()))
    }
    fn collision_up_face_full(&mut self, _: [i32; 3]) -> Result<bool, Error> {
        Err(Error("unexpected shape query".into()))
    }
    fn no_collision_no_liquid(&mut self, p: [i32; 3]) -> Result<bool, Error> {
        let dimensions = EntityType::PLAYER.data();
        let radius = f64::from(dimensions.width) / 2.0;
        let x = f64::from(p[0]) + 0.5;
        let y = f64::from(p[1]);
        let z = f64::from(p[2]) + 0.5;
        let top = y + f64::from(dimensions.height);
        self.calls
            .push([x - radius, y, z - radius, x + radius, top, z + radius]);
        Ok(y >= f64::from(self.floor + 1)
            && (self.ceiling < 0 || top <= f64::from(self.floor + self.ceiling)))
    }
}
#[test]
fn player_height_fixup_matches_vanilla_positions_and_collision_queries() {
    for case in golden(include_bytes!("golden/spawn_height_fixup.json.gz"))
        .as_array()
        .unwrap()
    {
        let mut world = CollisionWorld {
            floor: case["floor"].as_i64().unwrap() as i32,
            ceiling: case["ceiling"].as_i64().unwrap() as i32,
            calls: Vec::new(),
        };
        let result =
            fixup_spawn_height(&mut world, [-3, case["start"].as_i64().unwrap() as i32, 7])
                .unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), case["position"]);
        assert_eq!(serde_json::to_value(world.calls).unwrap(), case["calls"]);
    }
}
#[test]
fn player_candidates_and_random_draw_match_vanilla_schedule_next() {
    for case in golden(include_bytes!("golden/spawn_candidates.json.gz"))
        .as_array()
        .unwrap()
    {
        let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
        let mut finder = PlayerSpawnFinder::new(
            [-31, 64, 47],
            case["radius"].as_i64().unwrap() as i32,
            1e9,
            &mut random,
        );
        let requests = case["requests"].as_array().unwrap();
        for request in &requests[..requests.len() - 1] {
            assert_eq!(
                serde_json::to_value(finder.next_candidate().unwrap()).unwrap(),
                serde_json::json!([request[0], request[1]])
            );
        }
        assert_eq!(finder.next_candidate(), None);
        assert_eq!(random.next_long(), case["next"].as_i64().unwrap());
    }
}
