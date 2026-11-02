//! Attribute lookup timings against prepared vanilla defaults, with no rendering.
use horizon_world::{
    attribute_map::ValueKind,
    attributes::{
        entity::Catalog,
        live::{IdentityCounter, LiveAttributes},
    },
};
use std::{fs, hint::black_box, sync::Arc, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("attribute catalog path");
    let iterations: usize = args.next().unwrap_or_else(|| "20000000".into()).parse()?;
    let catalog = Arc::new(Catalog::from_json(&serde_json::from_slice(&fs::read(
        path,
    )?)?)?);
    let hashes: Vec<_> = catalog
        .attributes()
        .enumerate()
        .map(|(i, _)| i as i32)
        .collect();
    for cached in [false, true] {
        let mut attributes = LiveAttributes::new(
            Arc::clone(&catalog),
            "minecraft:player",
            &hashes,
            IdentityCounter::default(),
        )
        .expect("player attributes");
        let keys: Vec<_> = attributes
            .keys()
            .to_vec()
            .into_iter()
            .filter(|key| attributes.has_attribute(Some(key.clone())).unwrap())
            .collect();
        if cached {
            for key in &keys {
                attributes.get_instance(Some(key.clone())).unwrap();
            }
        }
        let queries: Vec<_> = (0..1024).map(|i| keys[i % keys.len()].clone()).collect();
        for i in 0..100_000 {
            black_box(
                attributes
                    .value(Some(queries[i & 1023].clone()), ValueKind::Computed)
                    .unwrap(),
            );
        }
        let start = Instant::now();
        let mut sum = 0.0;
        for i in 0..iterations {
            sum += black_box(
                attributes
                    .value(
                        Some(black_box(&queries[i & 1023]).clone()),
                        ValueKind::Computed,
                    )
                    .unwrap(),
            );
        }
        let elapsed = start.elapsed();
        println!(
            "{}",
            serde_json::json!({
                "case": if cached { "cached" } else { "supplier" },
                "attributes": keys.len(),
                "lookups": iterations,
                "ns_per_lookup": elapsed.as_nanos() as f64 / iterations as f64,
                "checksum": black_box(sum).to_bits(),
            })
        );
    }
    Ok(())
}
