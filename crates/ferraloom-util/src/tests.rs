use crate::{noise::*, random::*, vanilla_vectors as expected, *};

fn source(algorithm: usize, seed: i64) -> Random {
    match algorithm {
        0 => Random::Legacy(LegacyRandomSource::new(seed)),
        1 => Random::SingleThreaded(SingleThreadedRandomSource::new(seed)),
        _ => Random::Xoroshiro(XoroshiroRandomSource::new(seed)),
    }
}
fn assert_double(actual: f64, bits: u64) {
    let expected = f64::from_bits(bits);
    if expected.is_nan() {
        assert!(actual.is_nan());
    } else {
        assert_eq!(
            actual.to_bits(),
            bits,
            "actual {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn random_streams_match_vanilla_including_rejection_and_gaussian_cache() {
    for &(seed, algorithm, ints, longs, doubles, floats, bounded) in &expected::RNG {
        let mut r = source(algorithm, seed);
        assert_eq!(
            std::array::from_fn::<_, 8, _>(|_| r.next_int()),
            ints,
            "seed={seed}, algorithm={algorithm}"
        );
        r.set_seed(seed);
        assert_eq!(std::array::from_fn::<_, 4, _>(|_| r.next_long()), longs);
        r.set_seed(seed);
        let actual = [
            r.next_double(),
            r.next_gaussian(),
            r.next_gaussian(),
            r.triangle(-2.5, 3.0),
            r.next_double(),
            r.next_gaussian(),
            r.next_gaussian(),
            r.next_double(),
        ];
        for (actual, expected) in actual.into_iter().zip(doubles) {
            assert_double(actual, expected);
        }
        r.set_seed(seed);
        let actual = [
            r.next_float(),
            r.next_float(),
            r.triangle_f32(-2.5, 3.0),
            r.next_float(),
        ];
        assert_eq!(actual.map(f32::to_bits), floats);
        r.set_seed(seed);
        let bounds = [1, 2, 3, 1073741825, i32::MAX, 17, 1024, 1073741825];
        assert_eq!(bounds.map(|bound| r.next_int_bound(bound)), bounded);
        // Leave a Gaussian cached, then reseed: ordinary sources must discard it.
        r.next_gaussian();
        r.set_seed(seed);
        r.next_double();
        assert_double(r.next_gaussian(), doubles[1]);
    }
}
#[test]
fn positional_factories_and_forks_match_vanilla() {
    for &(seed, algorithm, values) in &expected::POSITIONAL {
        let mut r = source(algorithm, seed);
        let f = r.fork_positional();
        assert_eq!(
            [
                f.at(-30000000, -2048, 33554431).next_long(),
                f.at(0, 0, 0).next_long(),
                f.from_hash_of("minecraft:overworld").next_long(),
                f.from_hash_of("😀é").next_long(),
                f.from_seed(-1).next_long(),
                r.fork().next_long()
            ],
            values
        );
    }
}
#[test]
fn worldgen_seeds_preserve_int_overflow_and_source_specific_bit_consumption() {
    for &(seed, algorithm, decoration, values, count) in &expected::WORLDGEN {
        let mut r = WorldgenRandom::new(source(algorithm, seed));
        assert_eq!(r.set_decoration_seed(seed, -12345, 98765), decoration);
        let first = r.next_long();
        r.set_feature_seed(decoration, -7, 300000);
        let second = r.next_long();
        r.set_large_feature_seed(seed, -30000000, 30000000);
        let third = r.next_long();
        r.set_large_feature_with_salt(seed, -17, 99999, -123);
        let fourth = r.next_long();
        assert_eq!(
            [
                first,
                second,
                third,
                fourth,
                seed_slime_chunk(-30000000, 30000000, seed, 987234911).next_long()
            ],
            values
        );
        assert_eq!(r.count(), count);
    }
}
#[test]
fn noise_primitives_octaves_and_derivatives_match_26_3_float_precision() {
    const POINTS: [[f64; 3]; 5] = [
        [0.0, 0.0, 0.0],
        [-0.125, -64.75, 17.3],
        [30000000.25, -30000000.75, 16777216.0],
        [-33554432.1, 0.000000001, 33554431.9],
        [1.25e12, -1.5e12, 1234.5],
    ];
    for &(seed, algorithm, point, values) in &expected::NOISE {
        let p = PerlinNoise::new(&mut source(algorithm, seed));
        let s = SimplexNoise::new(&mut source(algorithm, seed));
        let so = SimplexNoise::with_discarded_offset(&mut source(algorithm, seed), true);
        let n = NormalNoise::new(NoiseParameters {
            base_amplitude: 1.7,
            base_octave: -5,
            octave_count: 4,
            amplitude_modifiers: vec![1.0, 0.0, 0.3, 1.0],
            ..NoiseParameters::default()
        });
        let normal = n.create(&mut source(algorithm, seed));
        let parity = NormalNoise::create_parity(-3, vec![1.0, 0.0, 0.5])
            .create(&mut source(algorithm, seed));
        let legacy = n.create_for_legacy_nether_biome(&mut source(algorithm, seed));
        let [x, y, z] = POINTS[point];
        let mut derivative = [0.25, -0.5, 1.5];
        let value = p.noise_with_derivative(x, y, z, &mut derivative);
        assert_eq!(value.to_bits(), p.get(x, y, z).to_bits());
        let actual = [
            p.get(x, y, z),
            p.get_2d(x, y),
            s.get(x, y, z),
            s.get_2d(x, y),
            so.get(x, y, z),
            normal.get(x, y, z),
            parity.get(x, y, z),
            legacy.get(x, y, z),
            derivative[0],
            derivative[1],
            derivative[2],
        ];
        assert_eq!(
            actual.map(f32::to_bits),
            values,
            "seed={seed}, algorithm={algorithm}, point={point}"
        );
    }
}
#[test]
fn position_packing_matches_vanilla_at_signed_extremes() {
    for &(input, packed, unpacked, section_packed, section_unpacked, chunk_packed) in
        &expected::PACKING
    {
        let [x, y, z] = input;
        let block = BlockPos::new(x, y, z);
        let section = SectionPos::new(x, y, z);
        assert_eq!(block.as_long(), packed);
        assert_eq!(
            BlockPos::of(packed),
            BlockPos::new(unpacked[0], unpacked[1], unpacked[2])
        );
        assert_eq!(section.as_long(), section_packed);
        assert_eq!(
            SectionPos::of(section_packed),
            SectionPos::new(
                section_unpacked[0],
                section_unpacked[1],
                section_unpacked[2]
            )
        );
        let chunk = ChunkPos::new(x, z);
        assert_eq!(chunk.pack(), chunk_packed);
        assert_eq!(ChunkPos::unpack(chunk_packed), chunk);
    }
    let actual: Vec<_> = BlockPos::between_closed(BlockPos::new(1, 2, 3), BlockPos::new(-1, 1, 2))
        .map(|p| [p.x, p.y, p.z])
        .collect();
    assert_eq!(actual, expected::BETWEEN);
}
#[test]
fn aabb_clipping_matches_vanilla_face_and_endpoint_rules() {
    let aabb = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    let rays = [
        (Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5)),
        (Vec3::new(0.5, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5)),
        (Vec3::new(-1.0, 0.5, 0.5), Vec3::new(0.0, 0.5, 0.5)),
        (Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 2.0, 2.0)),
        (Vec3::new(0.0, 0.5, 0.5), Vec3::new(1.0, 0.5, 0.5)),
        (
            Vec3::new(-1.0, 1.00000005, 0.5),
            Vec3::new(2.0, 1.00000005, 0.5),
        ),
        (
            Vec3::new(-1.0, 1.0000002, 0.5),
            Vec3::new(2.0, 1.0000002, 0.5),
        ),
    ];
    for ((from, to), expected) in rays.into_iter().zip(expected::AABB_CLIPS) {
        assert_eq!(
            aabb.clip(from, to)
                .map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]),
            expected
        );
    }
}
#[test]
fn math_tables_casts_and_wrapping_match_vanilla() {
    for &(input, floor, ceil, lfloor, sin, cos, inverse, wrap, modulo) in &expected::MATH {
        let x = f64::from_bits(input);
        assert_eq!(mth::floor(x), floor);
        assert_eq!(mth::ceil(x), ceil);
        assert_eq!(mth::lfloor(x), lfloor);
        assert_eq!(mth::sin(x).to_bits(), sin);
        assert_eq!(mth::cos(x).to_bits(), cos);
        assert_double(mth::fast_inv_sqrt(x), inverse);
        assert_double(mth::wrap_degrees(x), wrap);
        assert_double(mth::positive_modulo(x, 7.25), modulo);
    }
    for &(input, seed, mixed) in &expected::SEED_HASH {
        assert_eq!(mth::get_seed(input[0], input[1], input[2]), seed);
        assert_eq!(mth::murmur_hash3_mixer(input[0]), mixed);
    }
}
#[test]
fn java_string_md5_and_uuid_word_order_match_vanilla() {
    for &(name, hash, seed, uuid) in &expected::HASH {
        assert_eq!(java::string_hash_code(name), hash);
        let actual = seed_from_hash_of(name);
        assert_eq!([actual.seed_lo, actual.seed_hi], seed);
        let id = Uuid::offline_player(name);
        assert_eq!(id.to_int_array(), uuid);
        assert_eq!(Uuid::from_int_array(uuid), id);
        assert_eq!(Uuid::from_byte_array(id.to_byte_array()), id);
    }
}
