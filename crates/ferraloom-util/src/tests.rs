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

#[test]
fn directions_rotations_and_nearest_ties_match_vanilla() {
    for (d, expected) in Direction::VALUES.into_iter().zip(expected::DIRECTIONS) {
        let n = d.step();
        assert_eq!(
            [
                d.data_3d(),
                d.data_2d(),
                d.opposite().data_3d(),
                d.clockwise_axis(Axis::X).data_3d(),
                d.clockwise_axis(Axis::Y).data_3d(),
                d.clockwise_axis(Axis::Z).data_3d(),
                d.counter_clockwise_axis(Axis::X).data_3d(),
                d.counter_clockwise_axis(Axis::Y).data_3d(),
                d.counter_clockwise_axis(Axis::Z).data_3d(),
                Rotation::Clockwise90.rotate(d).data_3d(),
                Rotation::Clockwise180.rotate(d).data_3d(),
                Rotation::CounterClockwise90.rotate(d).data_3d(),
                Mirror::LeftRight.mirror(d).data_3d(),
                Mirror::FrontBack.mirror(d).data_3d(),
                n.x,
                n.y,
                n.z
            ],
            expected
        );
    }
    for &([x, y, z], nearest, approximate) in &expected::NEAREST {
        assert_eq!(
            Direction::get_nearest(x, y, z, None).map_or(-1, Direction::data_3d),
            nearest
        );
        assert_eq!(
            Direction::get_approximate_nearest(f64::from(x), f64::from(y), f64::from(z)).data_3d(),
            approximate
        );
    }
}
#[test]
fn java_division_and_masked_shifts_match_vanilla() {
    for &([a, b], expected) in &expected::DIVISION {
        assert_eq!(
            [
                java::floor_div(a, b),
                java::floor_mod(a, b),
                java::shr(a, b),
                java::ushr(a, b),
                java::shl(a, b),
                mth::positive_ceil_div(a, b),
                mth::round_toward(a, b),
                mth::wrap_degrees_int(a)
            ],
            expected
        );
    }
}
#[test]
fn interpolation_and_approximate_trigonometry_match_vanilla_rounding() {
    for &(input, doubles, floats, ints) in &expected::INTERPOLATION {
        let x = f64::from_bits(input);
        let f = x as f32;
        let actual = [
            mth::lerp(x, -7.3, 11.1),
            mth::clamped_lerp(x, -7.3, 11.1),
            mth::inverse_lerp(x, -0.3, 2.7),
            mth::map(x, -0.3, 2.7, -7.3, 11.1),
            mth::clamped_map(x, -0.3, 2.7, -7.3, 11.1),
            mth::rot_lerp(x, 179.3, -179.7),
        ];
        for (actual, expected) in actual.into_iter().zip(doubles) {
            assert_double(actual, expected);
        }
        let actual = [
            mth::lerp_f32(f, -7.3, 11.1),
            mth::clamped_lerp_f32(f, -7.3, 11.1),
            mth::inverse_lerp_f32(f, -0.3, 2.7),
            mth::smoothstep_f32(f),
            mth::smoothstep_derivative_f32(f),
            mth::fast_inv_cube_root(f),
            mth::inv_sqrt_f32(f),
            mth::rot_lerp_rad(f, 3.1, -3.1),
            mth::catmullrom(f, -3.0, 7.0, -11.0, 13.0),
        ];
        for (actual, expected) in actual.into_iter().zip(floats) {
            if f32::from_bits(expected).is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), expected, "input={f}");
            }
        }
        assert_eq!(
            [
                mth::lerp_int(f, i32::MAX, -98765),
                mth::lerp_discrete(f, 7, -11)
            ],
            ints
        );
    }
    for &([yb, xb], expected) in &expected::ATAN2 {
        assert_double(mth::atan2(f64::from_bits(yb), f64::from_bits(xb)), expected);
    }
}
#[test]
fn vector_operations_and_box_normalization_match_vanilla() {
    let v = Vec3::new(-12.25, 0.000001, 30000000.3);
    let w = Vec3::new(3.7, -9.1, 11.2);
    let actual = [
        v.normalize(),
        Vec3::new(0.0000001, 0.0, 0.0).normalize(),
        v.cross(w),
        v.lerp(w, 0.37),
        v.x_rot(0.37),
        v.y_rot(0.37),
        v.z_rot(0.37),
        Vec3::direction_from_rotation(-37.25, 179.3),
        v.projected_on(w),
    ];
    for (v, expected) in actual.into_iter().zip(expected::VECTORS) {
        assert_eq!([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()], expected);
    }
    let b = Aabb::new(1.0, -2.0, 3.0, 0.0, 4.0, -5.0);
    let c = Aabb::new(7.0, 8.0, 9.0, 11.0, 13.0, 17.0);
    let actual = [
        b,
        b.intersect(c),
        b.inflate(-3.0, 0.3, 1.1),
        b.move_by(0.1, -1e10, 17.3),
        b.contract(-3.0, 0.3, 1.1),
        b.expand_towards(-3.0, 0.3, 1.1),
        Aabb::new(-0.0, -0.0, -0.0, 1.0, 1.0, 1.0).expand_towards(0.0, f64::NAN, -0.0),
        Aabb::new(-0.0, -0.0, -0.0, 1.0, 1.0, 1.0).contract(0.0, f64::NAN, -0.0),
    ];
    for (b, expected) in actual.into_iter().zip(expected::BOXES) {
        assert_eq!(
            [
                b.min_x.to_bits(),
                b.min_y.to_bits(),
                b.min_z.to_bits(),
                b.max_x.to_bits(),
                b.max_y.to_bits(),
                b.max_z.to_bits()
            ],
            expected
        );
    }
}
#[test]
fn xoroshiro_zero_seed_and_noise_normalization_match_vanilla() {
    for &([lo, hi], expected) in &expected::RAW_XOROSHIRO {
        let mut r = Xoroshiro128PlusPlus::new(lo, hi);
        assert_eq!(std::array::from_fn::<_, 6, _>(|_| r.next_long()), expected);
    }
    for (mode, expected) in expected::NOISE_NORMALIZATION.into_iter().enumerate() {
        let n = NormalNoise::new(NoiseParameters {
            base_amplitude: 1.7,
            base_octave: -5,
            octave_count: 4,
            amplitude_modifiers: if mode == 3 {
                vec![0.0; 4]
            } else {
                vec![1.0, 0.0, 0.3, 1.0]
            },
            normalize: match mode {
                0 => Normalization::Disabled,
                2 => Normalization::Legacy,
                _ => Normalization::Enabled,
            },
        });
        let stack = n.create(&mut XoroshiroRandomSource::new(-1));
        let [nmin, nmax] = n.range();
        let [smin, smax] = stack.range();
        assert_eq!(
            [
                nmin,
                nmax,
                smin,
                smax,
                stack.get(-33554432.1, 0.000000001, 33554431.9)
            ]
            .map(f32::to_bits),
            expected
        );
    }
}

#[test]
fn volume_noise_and_smeared_perlin_match_vanilla_float_bits() {
    const POINTS: [[f64; 3]; 5] = [
        [0.0, 0.0, 0.0],
        [-0.125, -64.75, 17.3],
        [30000000.25, -30000000.75, 16777216.0],
        [-33554432.1, 0.000000001, 33554431.9],
        [1.25e12, -1.5e12, 1234.5],
    ];

    let volume = NoiseVolume {
        size: [2, 3, 2],
        min: [-33554431, -65, 16777215],
        step: [17, 3, 5],
    };
    for &(seed, algorithm, kind, expected, scalar) in &expected::NOISE_VOLUME {
        let mut random = source(algorithm, seed);
        let noise: Box<dyn Noise> = match kind {
            0 => Box::new(PerlinNoise::new(&mut random)),
            1 => Box::new(SmearedPerlinNoise::new(&mut random, 0.731)),
            2 => Box::new(SimplexNoise::new(&mut random)),
            3 => Box::new(
                NormalNoise::new(NoiseParameters {
                    base_octave: -5,
                    octave_count: 4,
                    amplitude_modifiers: vec![1.0, 0.0, 1.0, 1.0],
                    ..NoiseParameters::default()
                })
                .create(&mut random),
            ),
            _ => {
                let mut stack = NoiseStack::new();
                let mut frequency = 1.0;
                let mut amplitude = 0.9999847412109375 / (2.0_f64.powi(16) - 1.0);
                for _ in 0..16 {
                    stack.add(
                        SmearedPerlinNoise::new(&mut random, 684.412 * 0.125 * 8.0 * frequency),
                        frequency,
                        amplitude as f32,
                    );
                    frequency /= 2.0;
                    amplitude *= 2.0;
                }
                Box::new(stack)
            }
        };
        let mut buffer = vec![0.25; volume.len()];
        noise.add_to_volume(&mut buffer, volume, 0.33, 0.77, -0.37);
        assert_eq!(
            buffer.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            expected[..12],
            "first volume seed={seed} algorithm={algorithm} kind={kind}"
        );
        noise.add_to_volume(&mut buffer, volume, 0.11, 0.0, 0.125);
        assert_eq!(
            buffer.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            expected[12..],
            "second volume seed={seed} algorithm={algorithm} kind={kind}"
        );
        for (point, bits) in POINTS.into_iter().zip(scalar) {
            assert_eq!(
                noise.get(point[0], point[1], point[2]).to_bits(),
                bits,
                "scalar seed={seed} kind={kind}"
            );
        }
    }
}
