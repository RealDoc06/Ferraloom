//! Entity geometry helpers operating on an already selected collider list.
use super::{Aabb, Axis, COLLISION_EPSILON, Vec3, VoxelShape, java_max, java_min};
use crate::position::BlockPos;

/// What `Entity.collide` stops at: the shape's `collide` and its Y
/// coordinates (the step-up candidates).
pub trait Collides {
    fn collide(&self, axis: Axis, aabb: Aabb, amount: f64) -> f64;
    /// `getCoords(Axis.Y)`, ascending.
    fn y_coordinates(&self) -> impl Iterator<Item = f64> + '_;
}
impl Collides for VoxelShape {
    fn collide(&self, axis: Axis, aabb: Aabb, amount: f64) -> f64 {
        VoxelShape::collide(self, axis, aabb, amount)
    }
    fn y_coordinates(&self) -> impl Iterator<Item = f64> + '_ {
        self.coordinates(Axis::Y).iter().copied()
    }
}
impl<T: Collides> Collides for &T {
    fn collide(&self, axis: Axis, aabb: Aabb, amount: f64) -> f64 {
        (**self).collide(axis, aabb, amount)
    }
    fn y_coordinates(&self) -> impl Iterator<Item = f64> + '_ {
        (**self).y_coordinates()
    }
}

/// A collider of a block collision scan: a moved shape, or a block's unit
/// cube left as its position. `Shapes.block().move(p)` is an array shape
/// with the coordinates `[p, p + 1]` on each axis; `Block` answers exactly
/// as that shape does without building it.
#[derive(Clone, Debug)]
pub enum Collider {
    Shape(VoxelShape),
    Block(BlockPos),
}
impl Collider {
    /// The shape this collider stands for.
    pub fn into_shape(self) -> VoxelShape {
        match self {
            Self::Shape(shape) => shape,
            Self::Block(p) => VoxelShape::block().moved(block_offset(p)),
        }
    }
}
fn block_offset(p: BlockPos) -> Vec3 {
    Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z))
}
/// The moved unit cube's coordinates on an axis: `0 + p` and `1 + p`, as
/// `VoxelShape.move` adds the offset to each.
fn block_coords(p: BlockPos, axis: Axis) -> (f64, f64) {
    let c = f64::from([p.x, p.y, p.z][axis as usize]);
    (0.0 + c, 1.0 + c)
}
/// `VoxelShape.findIndex` over two coordinates (the binary search of an
/// array shape): -1 below `lo`, 0 inside, 1 from `hi` up (NaN included).
fn block_index((lo, hi): (f64, f64), value: f64) -> i32 {
    if value < hi {
        if value < lo { -1 } else { 0 }
    } else {
        1
    }
}
impl Collides for Collider {
    fn collide(&self, axis: Axis, aabb: Aabb, amount: f64) -> f64 {
        let p = match self {
            Self::Shape(shape) => return shape.collide(axis, aabb, amount),
            Self::Block(p) => *p,
        };
        // VoxelShape.collideX over the single full cell of a moved block.
        if amount.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        let i = axis as usize;
        let u = Axis::ALL[(i + 1) % 3];
        let v = Axis::ALL[(i + 2) % 3];
        let (cu, cv) = (block_coords(p, u), block_coords(p, v));
        let occupied = block_index(cu, aabb.min(u) + COLLISION_EPSILON).max(0)
            < (block_index(cu, aabb.max(u) - COLLISION_EPSILON) + 1).min(1)
            && block_index(cv, aabb.min(v) + COLLISION_EPSILON).max(0)
                < (block_index(cv, aabb.max(v) - COLLISION_EPSILON) + 1).min(1);
        let (lo, hi) = block_coords(p, axis);
        if amount > 0.0 {
            if occupied && block_index((lo, hi), aabb.max(axis) - COLLISION_EPSILON) == -1 {
                let distance = lo - aabb.max(axis);
                return if distance >= -COLLISION_EPSILON {
                    java_min(amount, distance)
                } else {
                    amount
                };
            }
        } else if amount < 0.0
            && occupied
            && block_index((lo, hi), aabb.min(axis) + COLLISION_EPSILON) >= 1
        {
            let distance = hi - aabb.min(axis);
            return if distance <= COLLISION_EPSILON {
                java_max(amount, distance)
            } else {
                amount
            };
        }
        amount
    }
    fn y_coordinates(&self) -> impl Iterator<Item = f64> + '_ {
        let (shape, block) = match self {
            Self::Shape(shape) => (Some(shape.coordinates(Axis::Y)), None),
            Self::Block(p) => {
                let (lo, hi) = block_coords(*p, Axis::Y);
                (None, Some([lo, hi]))
            }
        };
        shape
            .into_iter()
            .flatten()
            .copied()
            .chain(block.into_iter().flatten())
    }
}

/// BlockGetter's floor-height decision for caller-selected collision shapes.
/// The lower block is queried lazily only if the current shape is empty.
pub fn block_floor_height<'s>(shape: &VoxelShape, below: impl FnOnce() -> &'s VoxelShape) -> f64 {
    if !shape.is_empty() {
        return shape.max(Axis::Y);
    }
    let height = below().max(Axis::Y);
    if height >= 1.0 {
        height - 1.0
    } else {
        f64::NEG_INFINITY
    }
}

/// Bounds used once by Entity.collide to select entity collisions. The same
/// entity list is reused for both subsequent block/world-border queries.
pub fn entity_collision_bounds(movement: Vec3, aabb: Aabb, max_step: f32) -> Aabb {
    aabb.expand_towards(movement)
        .expand_towards(Vec3::new(0.0, max_step as f64, 0.0))
}

/// Resolve Entity.collide's initial movement and complete step-up decision.
///
/// `collect` supplies vanilla-equivalent colliders for each requested bound,
/// including the previously selected entity shapes, applicable world border,
/// and context-dependent block shapes in vanilla order. It is called for the
/// initial movement (unless its squared length is zero), then at most once
/// more if stepping is eligible. Entity collision selection and movement's
/// grounding/velocity/inside-block side effects belong to the caller.
pub fn resolve_step_movement<'s>(
    movement: Vec3,
    aabb: Aabb,
    max_step: f32,
    on_ground: bool,
    mut collect: impl FnMut(Aabb) -> Vec<&'s VoxelShape>,
) -> Vec3 {
    try_resolve_step_movement(movement, aabb, max_step, on_ground, |bounds| {
        Ok::<_, std::convert::Infallible>(collect(bounds))
    })
    .unwrap_or_else(|never| match never {})
}

/// The same movement/step decision with fallible, owned or borrowed collider
/// collections. A provider failure stops before the next query and returns no
/// movement result. Existing world-query side effects are not rolled back.
pub fn try_resolve_step_movement<S: Collides, E>(
    movement: Vec3,
    aabb: Aabb,
    max_step: f32,
    on_ground: bool,
    mut collect: impl FnMut(Aabb) -> Result<Vec<S>, E>,
) -> Result<Vec3, E> {
    try_resolve_step_movement_in(
        movement,
        aabb,
        max_step,
        on_ground,
        &mut Vec::new(),
        |bounds, shapes| {
            *shapes = collect(bounds)?;
            Ok(())
        },
    )
}

/// [`try_resolve_step_movement`] collecting into `shapes`, which `collect`
/// finds empty for each query: a caller can keep one buffer across moves.
pub fn try_resolve_step_movement_in<S: Collides, E>(
    movement: Vec3,
    aabb: Aabb,
    max_step: f32,
    on_ground: bool,
    shapes: &mut Vec<S>,
    mut collect: impl FnMut(Aabb, &mut Vec<S>) -> Result<(), E>,
) -> Result<Vec3, E> {
    let mut query = |bounds, shapes: &mut Vec<S>| {
        shapes.clear();
        collect(bounds, shapes)
    };
    let resolved = if movement.length_squared() == 0.0 {
        movement
    } else {
        query(aabb.expand_towards(movement), shapes)?;
        resolve_borrowed_shape_movement(movement, aabb, shapes)
    };
    let landed = movement.y != resolved.y && movement.y < 0.0;
    if max_step > 0.0
        && (landed || on_ground)
        && (movement.x != resolved.x || movement.z != resolved.z)
    {
        let step_box = if landed {
            aabb.move_by(Vec3::new(0.0, resolved.y, 0.0))
        } else {
            aabb
        };
        let mut bounds =
            step_box.expand_towards(Vec3::new(movement.x, max_step as f64, movement.z));
        if !landed {
            bounds = bounds.expand_towards(Vec3::new(0.0, -(1.0e-5_f32 as f64), 0.0));
        }
        query(bounds, shapes)?;
        for height in borrowed_step_heights(step_box, shapes, max_step, resolved.y as f32) {
            let candidate = resolve_borrowed_shape_movement(
                Vec3::new(movement.x, height as f64, movement.z),
                step_box,
                shapes,
            );
            if candidate.horizontal_distance_squared() > resolved.horizontal_distance_squared() {
                return Ok(candidate.subtract(Vec3::new(
                    0.0,
                    aabb.min(Axis::Y) - step_box.min(Axis::Y),
                    0.0,
                )));
            }
        }
    }
    Ok(resolved)
}

/// Resolve vanilla's Y-first, magnitude-dependent horizontal axis ordering.
///
/// The caller must supply vanilla-equivalent block/entity/world-border shapes.
/// This helper does not collect shapes, step up, set grounded flags, modify
/// velocity, or apply inside-block effects.
pub fn resolve_shape_movement(movement: Vec3, aabb: Aabb, shapes: &[&VoxelShape]) -> Vec3 {
    resolve_borrowed_shape_movement(movement, aabb, shapes)
}
fn resolve_borrowed_shape_movement<S: Collides>(movement: Vec3, aabb: Aabb, shapes: &[S]) -> Vec3 {
    if shapes.is_empty() {
        return movement;
    }
    let mut result = [0.0; 3];
    for axis in Axis::step_order(movement) {
        let amount = movement.component(axis);
        if amount == 0.0 {
            continue;
        }
        let moved = aabb.move_by(Vec3::new(result[0], result[1], result[2]));
        result[axis as usize] = collide_all(axis, moved, shapes, amount);
    }
    Vec3::new(result[0], result[1], result[2])
}

/// Candidate heights from Entity.collectCandidateStepUpHeights, including its
/// float narrowing, signed-zero set identity, current-Y exclusion and sorting.
/// Used by the step-up decision after collecting the appropriate world shapes.
pub fn candidate_step_heights(
    aabb: Aabb,
    shapes: &[&VoxelShape],
    max_step: f32,
    current_y: f32,
) -> Vec<f32> {
    borrowed_step_heights(aabb, shapes, max_step, current_y)
}

/// `Shapes.collide`: each shape in turn, until the movement is spent.
fn collide_all<S: Collides>(axis: Axis, aabb: Aabb, shapes: &[S], mut amount: f64) -> f64 {
    for shape in shapes {
        if amount.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        amount = shape.collide(axis, aabb, amount);
    }
    amount
}

fn borrowed_step_heights<S: Collides>(
    aabb: Aabb,
    shapes: &[S],
    max_step: f32,
    current_y: f32,
) -> Vec<f32> {
    let mut heights = Vec::<f32>::with_capacity(4);
    for shape in shapes {
        for coordinate in shape.y_coordinates() {
            let height = (coordinate - aabb.min(Axis::Y)) as f32;
            if height < 0.0 || height == current_y {
                continue;
            }
            if height > max_step {
                break;
            }
            // FloatArraySet uses Float.floatToIntBits, so NaNs compare equal
            // while positive and negative zero remain separate members.
            let height = if height.is_nan() { f32::NAN } else { height };
            if !heights.iter().any(|old| old.to_bits() == height.to_bits()) {
                heights.push(height);
            }
        }
    }
    heights.sort_unstable_by(f32::total_cmp);
    heights
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::BlockPos;
    #[test]
    fn movement_slides_after_a_wall_collision() {
        let wall = VoxelShape::block();
        let b = Aabb::new(Vec3::new(-0.6, 0.0, 0.2), Vec3::new(-0.2, 1.8, 0.8));
        let requested = Vec3::new(1.0, 0.0, 0.1);
        assert_eq!(
            resolve_shape_movement(requested, b, &[&wall]),
            Vec3::new(0.2, 0.0, 0.1)
        );
        assert_eq!(
            resolve_shape_movement(Vec3::new(-0.0, -0.0, -0.0), b, &[]),
            Vec3::new(-0.0, -0.0, -0.0)
        );
        assert_eq!(
            resolve_shape_movement(Vec3::new(-0.0, -0.0, -0.0), b, &[&wall]),
            Vec3::ZERO
        );
    }
    #[test]
    fn step_heights_deduplicate_and_exclude_current_y() {
        let shape = VoxelShape::from_cube([1, 2, 1], &[true, true]).unwrap();
        let b = Aabb::block(BlockPos::default());
        assert_eq!(
            candidate_step_heights(b, &[&shape, &shape], 0.6, 0.0),
            vec![0.5]
        );
        assert_eq!(
            candidate_step_heights(b, &[&shape], 1.0, 0.5),
            vec![0.0, 1.0]
        );
    }

    #[test]
    fn stepping_requeries_and_clears_a_half_block() {
        let ledge = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))).unwrap();
        let b = Aabb::new(Vec3::new(-0.6, 0.0, 0.2), Vec3::new(-0.2, 1.8, 0.8));
        let requested = Vec3::new(1.0, 0.0, 0.0);
        let mut queries = Vec::new();
        let result = resolve_step_movement(requested, b, 0.6, true, |query| {
            queries.push(query);
            vec![&ledge]
        });
        assert_eq!(result, Vec3::new(1.0, 0.5, 0.0));
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0], b.expand_towards(requested));
        assert_eq!(queries[1].min(Axis::Y), -(1.0e-5_f32 as f64));
        assert_eq!(
            resolve_step_movement(requested, b, 0.6, false, |_| vec![&ledge]),
            Vec3::new(0.2, 0.0, 0.0)
        );
    }

    #[test]
    fn squared_length_underflow_skips_collection_and_keeps_signed_zero() {
        let b = Aabb::block(BlockPos::default());
        let requested = Vec3::new(-0.0, 1e-200, -0.0);
        assert_eq!(
            resolve_step_movement(requested, b, 0.6, true, |_| panic!("unexpected query")),
            requested
        );
    }
    #[test]
    fn floor_height_defers_the_lower_lookup_until_empty() {
        let empty = VoxelShape::empty();
        let block = VoxelShape::block();
        assert_eq!(
            block_floor_height(&block, || panic!("unnecessary lower lookup")),
            1.0
        );
        assert_eq!(block_floor_height(&empty, || &block), 0.0);
        assert_eq!(block_floor_height(&empty, || &empty), f64::NEG_INFINITY);
    }
}

#[cfg(test)]
mod fallible_tests {
    use super::*;
    #[test]
    fn owned_collision_queries_stop_at_first_failure() {
        let bounds = Aabb::new(Vec3::new(-0.6, 0.0, 0.2), Vec3::new(-0.2, 1.8, 0.8));
        let ledge = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))).unwrap();
        for fail in [0, 1] {
            let mut queries = 0;
            let result =
                try_resolve_step_movement(Vec3::new(1.0, 0.0, 0.0), bounds, 0.6, true, |_| {
                    let call = queries;
                    queries += 1;
                    if call == fail {
                        Err(call)
                    } else {
                        Ok(vec![ledge.clone()])
                    }
                });
            assert_eq!(result, Err(fail));
            assert_eq!(queries, fail + 1);
        }
        let result = try_resolve_step_movement(
            Vec3::new(-0.0, -0.0, -0.0),
            bounds,
            0.6,
            true,
            |_| -> Result<Vec<VoxelShape>, ()> {
                panic!("zero movement must skip shape collection")
            },
        )
        .unwrap();
        assert_eq!(
            [result.x.to_bits(), result.y.to_bits(), result.z.to_bits()],
            [(-0.0f64).to_bits(); 3]
        );
    }
    /// A block's collider answers as the moved unit cube, bit for bit, over
    /// boxes on and around its faces and edges.
    #[test]
    fn block_collider_matches_the_moved_unit_cube() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let offsets = [
            -1.0,
            -0.6,
            -1.0e-7,
            -5.0e-8,
            0.0,
            5.0e-8,
            1.0e-7,
            0.3,
            0.5,
            0.999_999_9,
            1.0,
            1.000_000_1,
            1.6,
            2.0,
        ];
        let amounts = [
            0.0,
            -0.0,
            5.0e-8,
            -5.0e-8,
            0.0784,
            -0.0784,
            0.6,
            -0.6,
            3.0,
            -3.0,
            f64::NAN,
        ];
        for p in [
            BlockPos::new(0, 0, 0),
            BlockPos::new(-3, 64, 17),
            BlockPos::new(-30_000_000, -64, 29_999_999),
        ] {
            let collider = Collider::Block(p);
            let shape = VoxelShape::block().moved(Vec3::new(
                f64::from(p.x),
                f64::from(p.y),
                f64::from(p.z),
            ));
            assert!(collider.y_coordinates().eq(shape.y_coordinates()));
            for _ in 0..20_000 {
                let mut pick = |base: i32| {
                    let r = next();
                    let o = if r % 4 == 0 {
                        (r >> 11) as f64 / (1u64 << 53) as f64 * 3.0 - 1.0
                    } else {
                        offsets[(r >> 8) as usize % offsets.len()]
                    };
                    f64::from(base) + o
                };
                let a = Vec3::new(pick(p.x), pick(p.y), pick(p.z));
                let b = Vec3::new(pick(p.x), pick(p.y), pick(p.z));
                let aabb = Aabb::new(a, b);
                let amount = amounts[next() as usize % amounts.len()];
                for axis in Axis::ALL {
                    assert_eq!(
                        Collides::collide(&collider, axis, aabb, amount).to_bits(),
                        shape.collide(axis, aabb, amount).to_bits(),
                        "{p:?} {axis:?} {aabb:?} {amount}"
                    );
                }
            }
        }
    }
}
