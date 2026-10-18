use super::*;
const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn vector(value: &str) -> Vec3 {
    let values: Vec<_> = value
        .split(',')
        .map(|v| f64::from_bits(u64::from_str_radix(v, 16).unwrap()))
        .collect();
    Vec3::new(values[0], values[1], values[2])
}
fn bits(position: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        position.x.to_bits(),
        position.y.to_bits(),
        position.z.to_bits()
    )
}
fn path_text(path: &PositionPath) -> String {
    match path {
        PositionPath::Linear(pos) => format!("L:{}", bits(Vec3::new(pos.x, pos.y, pos.z))),
        PositionPath::Stepped(steps) => format!(
            "S:{}",
            steps
                .iter()
                .map(|step| format!(
                    "{}:{}",
                    step.tick_offset.0,
                    bits(Vec3::new(step.position.x, step.position.y, step.position.z))
                ))
                .collect::<Vec<_>>()
                .join(";")
        ),
    }
}
fn path(value: &str) -> PositionPath {
    if let Some(pos) = value.strip_prefix("L:") {
        let position = vector(pos);
        return PositionPath::Linear(ferraloom_protocol::play::Vec3 {
            x: position.x,
            y: position.y,
            z: position.z,
        });
    }
    PositionPath::Stepped(
        value
            .strip_prefix("S:")
            .unwrap()
            .split(';')
            .map(|step| {
                let (ticks, position) = step.split_once(':').unwrap();
                let position = vector(position);
                PositionStep {
                    position: ferraloom_protocol::play::Vec3 {
                        x: position.x,
                        y: position.y,
                        z: position.z,
                    },
                    tick_offset: VarInt(ticks.parse().unwrap()),
                }
            })
            .collect(),
    )
}
fn delta_text(delta: Option<&VecDelta>) -> String {
    match delta {
        None => "null".into(),
        Some(VecDelta::Linear { xa, ya, za }) => format!("{xa},{ya},{za}"),
        Some(VecDelta::Stepped(steps)) => steps
            .iter()
            .map(|step| format!("{}:{},{},{}", step.ticks.0, step.xa, step.ya, step.za))
            .collect::<Vec<_>>()
            .join(";"),
    }
}

#[test]
fn vanilla_delta_rounding_limits_precision_loss_and_stepped_bases() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("delta-codec\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let codec = VecDeltaCodec { base: vector(f[1]) };
        let position = vector(f[2]);
        let delta = codec.try_encode(position);
        assert_eq!(delta_text(delta.as_ref()), f[3], "{row}");
        let decoded = if let Some(VecDelta::Linear { xa, ya, za }) = delta {
            codec.decode(i64::from(xa), i64::from(ya), i64::from(za))
        } else {
            codec.base
        };
        assert_eq!(bits(decoded), f[4], "{row}");
        let loss = Vec3::new(
            encoding_precision_loss(position.x),
            encoding_precision_loss(position.y),
            encoding_precision_loss(position.z),
        );
        assert_eq!(bits(loss), f[5], "{row}");
        count += 1;
    }
    assert_eq!(count, 15);
    let mut stepped = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("delta-steps\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let codec = VecDeltaCodec { base: vector(f[1]) };
        assert_eq!(
            delta_text(codec.try_encode_path(&path(f[2])).as_ref()),
            f[3],
            "{row}"
        );
        assert_eq!(bits(codec.base), f[4], "{row}");
        stepped += 1;
    }
    assert_eq!(stepped, 3);
}

#[test]
fn vanilla_stepped_tracking_respects_sync_position_predicted_delta_and_tick_offsets() {
    let mut tracker = SteppedInterpolationTracker::default();
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("tracked-steps\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let tick: i32 = f[1].parse().unwrap();
        let position = vector(f[2]);
        let mut sync = matches!(tick, 1 | 3 | 4 | 7);
        if tick == 4 {
            tracker.apply_predicted_movement(Vec3::new(0.1, 0.0, 0.0));
        }
        tracker.update_tracking(position, &mut sync);
        assert_eq!(sync, f[3].parse::<bool>().unwrap());
        if f[4] != "-" {
            assert_eq!(path_text(&tracker.position_path(position)), f[4], "{row}");
            tracker.clear();
        }
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn java_round_preserves_ties_neighbors_and_saturation() {
    assert_eq!(java_round(f64::from_bits(0.5_f64.to_bits() - 1)), 0);
    assert_eq!(java_round(0.5), 1);
    assert_eq!(java_round(-0.5), 0);
    assert_eq!(java_round(f64::from_bits((-0.5_f64).to_bits() + 1)), -1);
    assert_eq!(java_round(f64::INFINITY), i64::MAX);
    assert_eq!(java_round(f64::NEG_INFINITY), i64::MIN);
    assert_eq!(java_round(f64::NAN), 0);
}
