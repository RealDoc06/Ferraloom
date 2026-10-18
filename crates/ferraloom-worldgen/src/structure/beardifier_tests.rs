use super::*;
use crate::density::*;
use serde_json::Value;
use std::{io::Read, sync::Arc};

#[test]
fn scalar_and_volume_terrain_adjustments_match_vanilla_bits() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/beardifier.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    for case in cases.as_array().unwrap() {
        let bounds = BoundingBox::new([-3, 61, 2], [11, 76, 14]);
        let adjustment = TerrainAdjustment::decode(case["adjustment"].as_str().unwrap()).unwrap();
        let delta = case["delta"].as_i64().unwrap() as i32;
        let beard = Beardifier::new(
            vec![
                Rigid {
                    bounds,
                    adjustment,
                    ground_level_delta: delta,
                },
                Rigid {
                    bounds: BoundingBox::new([9, 58, 3], [17, 68, 18]),
                    adjustment: TerrainAdjustment::BeardBox,
                    ground_level_delta: 2,
                },
            ],
            vec![
                JigsawJunction {
                    source: [4, 65, -2],
                    delta_y: 1,
                    destination_projection: Projection::Rigid,
                },
                JigsawJunction {
                    source: [-9, 62, 7],
                    delta_y: -3,
                    destination_projection: Projection::TerrainMatching,
                },
            ],
            Some(bounds.inflated(24)),
        );
        let mut values = case["scalars"].as_array().unwrap().iter();
        for z in (-16..=30).step_by(2) {
            for x in (-18..=30).step_by(2) {
                for y in 45..=87 {
                    assert_eq!(
                        beard.sample_value([x, y, z]).to_bits(),
                        values.next().unwrap().as_u64().unwrap() as u32,
                        "{adjustment:?} delta={delta} at [{x},{y},{z}]"
                    );
                }
            }
        }
        assert!(values.next().is_none());
        for volume in case["volumes"].as_array().unwrap() {
            let v = DensityVolume {
                min: std::array::from_fn(|i| volume["min"][i].as_i64().unwrap() as i32),
                size: std::array::from_fn(|i| volume["size"][i].as_u64().unwrap() as usize),
                step: std::array::from_fn(|i| volume["step"][i].as_i64().unwrap() as i32),
            };
            for (i, (actual, expected)) in beard
                .sample_volume(v)
                .iter()
                .zip(volume["values"].as_array().unwrap())
                .enumerate()
            {
                assert_eq!(
                    actual.to_bits(),
                    expected.as_u64().unwrap() as u32,
                    "{adjustment:?} delta={delta} volume={v:?} at {:?}",
                    v.position(i)
                );
            }
        }
    }
}

#[test]
fn marker_is_job_local_and_replacing_it_invalidates_cached_wrappers() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Beardifier>();
    let mut compiler = DensityFunctionCompiler::new(Arc::new(DensityRegistry::default()), 0, false);
    let marker = DensityFunction::decode(
        &serde_json::json!({"type":"minecraft:cache","input":{"type":"minecraft:beardifier"}}),
    )
    .unwrap();
    let node = compiler.compile(&marker).unwrap();
    let p = [0, 63, 0];
    let bounds = BoundingBox::new([0, 60, 0], [5, 70, 5]);
    let beard = Arc::new(Beardifier::for_chunk(
        [0, 0],
        [TerrainPiece {
            bounds,
            adjustment: TerrainAdjustment::Bury,
            ground_level_delta: 0,
            projection: None,
            junctions: &[],
        }],
    ));
    let mut ctx = SamplerContext::cached();
    let mut independent = SamplerContext::cached();
    assert_eq!(compiler.sample_value(node, &mut ctx, p), 0.0);
    ctx.set_beardifier(Some(beard.clone()));
    assert_eq!(
        compiler.sample_value(node, &mut ctx, p).to_bits(),
        beard.sample_value(p).to_bits()
    );
    assert_ne!(beard.sample_value(p), 0.0);
    assert_eq!(compiler.sample_value(node, &mut independent, p), 0.0);
    let volume = DensityVolume {
        size: [2, 4, 3],
        min: [0, 61, 0],
        step: [1; 3],
    };
    assert_eq!(
        compiler.sample_volume(node, &mut ctx, volume),
        beard.sample_volume(volume)
    );
    ctx.set_beardifier(None);
    assert!(
        compiler
            .sample_volume(node, &mut ctx, volume)
            .iter()
            .all(|v| v.to_bits() == 0)
    );
}
