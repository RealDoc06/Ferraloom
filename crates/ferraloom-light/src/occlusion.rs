use ferraloom_data::{BLOCK_STATES, BlockState, SHAPES};
use ferraloom_util::core::{Axis, AxisDirection, Direction};
use std::sync::OnceLock;

type Face = [u64; 4];
static FACES: OnceLock<Vec<[Face; 6]>> = OnceLock::new();

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
    if shape_occludes(from, to, direction) {
        16
    } else {
        simple_opacity
    }
}

fn face(faces: &[[Face; 6]], state: BlockState, direction: Direction) -> Face {
    if is_empty_shape(state) {
        [0; 4]
    } else {
        faces[state.data().occlusion.id() as usize][direction as usize]
    }
}

fn build_faces() -> Vec<[Face; 6]> {
    let mut faces = vec![[[0; 4]; 6]; SHAPES.len()];
    for state in BLOCK_STATES
        .iter()
        .filter(|s| s.use_shape_for_light_occlusion)
    {
        let shape = state.occlusion;
        for b in shape.boxes() {
            for coordinate in b.min.into_iter().chain(b.max) {
                assert!(
                    (0.0..=1.0).contains(&coordinate)
                        && coordinate * 16.0 == (coordinate * 16.0).round(),
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
                for y in (b.min[v] * 16.0) as usize..(b.max[v] * 16.0) as usize {
                    for x in (b.min[u] * 16.0) as usize..(b.max[u] * 16.0) as usize {
                        let bit = y * 16 + x;
                        mask[bit / 64] |= 1 << (bit % 64);
                    }
                }
            }
            faces[shape.id() as usize][d as usize] = mask;
        }
    }
    faces
}
