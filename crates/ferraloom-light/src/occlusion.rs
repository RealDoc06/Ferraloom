use ferraloom_data::{BLOCK_STATES, BlockState, SHAPES};
use ferraloom_util::core::{Axis, AxisDirection, Direction};
use std::sync::OnceLock;

type Face = [u64; 4];
struct ShapeFaces {
    faces: [Face; 6],
    merged: [Face; 6],
}
static FACES: OnceLock<Vec<ShapeFaces>> = OnceLock::new();

/// Port of `LightEngine#isEmptyShape`. `getOcclusionShape` is already empty
/// for states with `canOcclude == false` in the generated registry facts.
pub fn is_empty_shape(state: BlockState) -> bool {
    !state.data().use_shape_for_light_occlusion || state.data().occlusion.boxes().is_empty()
}

/// Port of `LightEngine#shapeOccludes` and `Shapes#faceShapeOccludes`.
/// Every light-occluding 26.3 shape lies exactly on the sixteenth grid. Assert
/// this while building masks so new registry data cannot silently approximate.
pub fn shape_occludes(from: BlockState, to: BlockState, direction: Direction) -> bool {
    let faces = FACES.get_or_init(build_faces);
    let a = face(faces, from, direction);
    let b = face(faces, to, direction.opposite());
    a.into_iter().zip(b).all(|(a, b)| a | b == u64::MAX)
}

pub fn has_different_light_properties(old: BlockState, new: BlockState) -> bool {
    old != new
        && (old.data().opacity != new.data().opacity
            || old.data().light_emission != new.data().light_emission
            || old.data().use_shape_for_light_occlusion
            || new.data().use_shape_for_light_occlusion)
}

/// Port of `LightEngine#getLightDampeningInto` for the pinned registry shapes.
pub fn light_dampening_into(
    from: BlockState,
    to: BlockState,
    direction: Direction,
    simple_opacity: u8,
) -> u8 {
    let faces = FACES.get_or_init(build_faces);
    let a = merged_face(faces, from, direction);
    let b = merged_face(faces, to, direction.opposite());
    if a.into_iter().zip(b).all(|(a, b)| a | b == u64::MAX) {
        16
    } else {
        simple_opacity
    }
}

fn face(faces: &[ShapeFaces], state: BlockState, direction: Direction) -> Face {
    if is_empty_shape(state) {
        [0; 4]
    } else {
        faces[state.data().occlusion.id() as usize].faces[direction as usize]
    }
}

fn merged_face(faces: &[ShapeFaces], state: BlockState, direction: Direction) -> Face {
    if is_empty_shape(state) {
        [0; 4]
    } else {
        faces[state.data().occlusion.id() as usize].merged[direction as usize]
    }
}
fn build_faces() -> Vec<ShapeFaces> {
    let mut faces: Vec<_> = (0..SHAPES.len())
        .map(|_| ShapeFaces {
            faces: [[0; 4]; 6],
            merged: [[0; 4]; 6],
        })
        .collect();
    for state in BLOCK_STATES
        .iter()
        .filter(|s| s.use_shape_for_light_occlusion)
    {
        let shape = state.occlusion;
        for b in shape.boxes() {
            for coordinate in b.min.into_iter().chain(b.max) {
                assert!(
                    coordinate * 16.0 == (coordinate * 16.0).round(),
                    "light occlusion shape exceeds the exact sixteenth grid"
                );
            }
        }
        for d in Direction::VALUES {
            let axis = match d.axis() {
                Axis::X => 0,
                Axis::Y => 1,
                Axis::Z => 2,
            };
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            let slice = if d.axis_direction() == AxisDirection::Positive {
                0.9999999
            } else {
                1.0e-7
            };
            let mut mask = [0; 4];
            for b in shape
                .boxes()
                .iter()
                .filter(|b| b.min[axis] <= slice && b.max[axis] > slice)
            {
                for y in (b.min[v].clamp(0.0, 1.0) * 16.0) as usize
                    ..(b.max[v].clamp(0.0, 1.0) * 16.0) as usize
                {
                    for x in (b.min[u].clamp(0.0, 1.0) * 16.0) as usize
                        ..(b.max[u].clamp(0.0, 1.0) * 16.0) as usize
                    {
                        let bit = y * 16 + x;
                        mask[bit / 64] |= 1 << (bit % 64);
                    }
                }
            }
            faces[shape.id() as usize].faces[d as usize] = mask;
            let boundary = if d.axis_direction() == AxisDirection::Positive {
                shape
                    .boxes()
                    .iter()
                    .map(|b| b.max[axis])
                    .fold(f64::NEG_INFINITY, f64::max)
            } else {
                shape
                    .boxes()
                    .iter()
                    .map(|b| b.min[axis])
                    .fold(f64::INFINITY, f64::min)
            };
            let target = if d.axis_direction() == AxisDirection::Positive {
                1.0
            } else {
                0.0
            };
            if (boundary - target).abs() <= 1.0e-7 {
                faces[shape.id() as usize].merged[d as usize] = mask;
            }
        }
    }
    faces
}
