use super::*;
use ferraloom_util::random::{LegacyRandomSource, Random, XoroshiroRandomSource};
use std::io::Read;

#[test]
fn native_value_effects_keep_float_results_branch_boundaries_and_rng() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/enchantments/value-effects.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut cases = 0;
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let value: Value = serde_json::from_str(fields[0]).unwrap();
        let effect = EnchantmentValueEffect::decode(&value).unwrap();
        let level = fields[1].parse().unwrap();
        let input = f32::from_bits(fields[2].parse::<u32>().unwrap());
        let seed = fields[4].parse::<i64>().unwrap();
        let mut random = match fields[3] {
            "legacy" => Random::Legacy(LegacyRandomSource::new(seed)),
            "xoroshiro" => Random::Xoroshiro(XoroshiroRandomSource::new(seed)),
            _ => panic!("unknown native RNG"),
        };
        let result = effect.process(level, &mut random, input).unwrap();
        let expected = f32::from_bits(fields[5].parse::<u32>().unwrap());
        assert!(
            result.to_bits() == expected.to_bits() || result.is_nan() && expected.is_nan(),
            "{line}: {result:?} != {expected:?}"
        );
        assert_eq!(
            random.next_long(),
            fields[6].parse::<i64>().unwrap(),
            "{line}"
        );
        assert_eq!(
            random.next_gaussian().to_bits(),
            fields[7].parse::<u64>().unwrap(),
            "Gaussian cache {line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 13600);
}
