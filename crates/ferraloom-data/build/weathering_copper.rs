use serde_json::Value;
use std::fmt::Write;

pub fn generate(out: &mut String, report: &Value, block_count: usize) {
    assert_eq!(
        report["block_count"].as_u64().unwrap() as usize,
        block_count
    );
    let mut ages = vec![None; block_count];
    let mut next = vec![None; block_count];
    let mut previous = vec![None; block_count];
    for row in report["blocks"]
        .as_array()
        .expect("weathering copper facts")
    {
        let id = row["block"].as_u64().unwrap() as usize;
        let age = row["age"].as_u64().unwrap();
        assert!(age < 4);
        assert!(
            ages[id].replace(age).is_none(),
            "duplicate weathering copper"
        );
        if let Some(target) = row["next"].as_u64() {
            let target = target as usize;
            assert!(target < block_count);
            next[id] = Some(target);
            assert!(
                previous[target].replace(id).is_none(),
                "non-invertible weathering map"
            );
        }
    }
    writeln!(out, "static WEATHERING_COPPER_PROPERTIES: [Option<WeatheringCopperProperties>; {block_count}] = [").unwrap();
    for (id, age) in ages.iter().enumerate() {
        let Some(age) = age else {
            assert!(previous[id].is_none());
            out.push_str("None,\n");
            continue;
        };
        let variant = ["Unaffected", "Exposed", "Weathered", "Oxidized"][*age as usize];
        if let Some(target) = next[id] {
            assert_eq!(ages[target], Some(age + 1));
        }
        let mut first = id;
        let mut distance = 0;
        while let Some(parent) = previous[first] {
            first = parent;
            distance += 1;
            assert!(distance < 4, "cyclic weathering map");
        }
        assert_eq!(ages[first], Some(0));
        let block = |id: Option<usize>| {
            id.map_or_else(|| "None".to_owned(), |id| format!("Some(Block({id}))"))
        };
        writeln!(out, "Some(WeatheringCopperProperties {{ age: WeatherState::{variant}, next: {}, previous: {}, first: Block({first}) }}),", block(next[id]), block(previous[id])).unwrap();
    }
    out.push_str("];\n");
}
