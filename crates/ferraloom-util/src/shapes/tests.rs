use super::*;
use crate::BlockPos;

struct Reader<'a>(&'a [u8], Vec<&'a [u8]>);
impl<'a> Reader<'a> {
    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let (head, tail) = self.0.split_at(N);
        self.0 = tail;
        head.try_into().unwrap()
    }
    fn byte(&mut self) -> u8 {
        self.bytes::<1>()[0]
    }
    fn boolean(&mut self) -> bool {
        self.byte() != 0
    }
    fn int(&mut self) -> i32 {
        i32::from_be_bytes(self.bytes())
    }
    fn count(&mut self) -> usize {
        self.int().try_into().unwrap()
    }
    fn double(&mut self) -> f64 {
        f64::from_bits(u64::from_be_bytes(self.bytes()))
    }
    fn doubles<const N: usize>(&mut self) -> [f64; N] {
        std::array::from_fn(|_| self.double())
    }
    fn vec3(&mut self) -> Vec3 {
        let [x, y, z] = self.doubles();
        Vec3::new(x, y, z)
    }
    fn aabb(&mut self) -> Aabb {
        let [x, y, z, u, v, w] = self.doubles();
        Aabb::new(x, y, z, u, v, w)
    }
    fn shape(&mut self, shapes: &[VoxelShape]) -> VoxelShape {
        shapes[self.count()].clone()
    }
    fn lines(&mut self) -> Vec<[f64; 6]> {
        (0..self.count()).map(|_| self.doubles()).collect()
    }
    fn snapshot(&mut self) -> Snapshot {
        let index = self.count();
        if index == self.1.len() {
            let length = self.count();
            let (snapshot, rest) = self.0.split_at(length);
            self.1.push(snapshot);
            self.0 = rest;
        }
        let mut reader = Reader(self.1[index], Vec::new());
        let snapshot = reader.raw_snapshot();
        assert!(reader.0.is_empty());
        snapshot
    }
    fn raw_snapshot(&mut self) -> Snapshot {
        let kind = match self.byte() {
            0 => VoxelShapeKind::Cube,
            1 => VoxelShapeKind::Array,
            2 => VoxelShapeKind::Slice,
            _ => panic!("shape kind"),
        };
        let is_block = self.boolean();
        let is_empty_singleton = self.boolean();
        let is_empty = self.boolean();
        let mut size = [0; 3];
        let mut first = [0; 3];
        let mut last = [0; 3];
        let mut extrema = [[0.0; 2]; 3];
        let coords = std::array::from_fn(|a| {
            size[a] = self.int();
            first[a] = self.int();
            last[a] = self.int();
            let cube = self.boolean();
            let values: Vec<_> = (0..=size[a]).map(|_| self.double()).collect();
            extrema[a] = self.doubles();
            if cube {
                let coords = Coordinates::cube(size[a] as usize);
                for (i, v) in values.into_iter().enumerate() {
                    assert_bits(coords.get(i), v, "input cube coordinate");
                }
                coords
            } else {
                Coordinates::array(values)
            }
        });
        let mut discrete = BitSetDiscreteVoxelShape::new(size[0], size[1], size[2]);
        for x in 0..size[0] {
            for y in 0..size[1] {
                for z in 0..size[2] {
                    if self.boolean() {
                        discrete.fill(x, y, z);
                    }
                }
            }
        }
        discrete.set_bounds(first, last);
        Snapshot {
            kind,
            is_block,
            is_empty_singleton,
            is_empty,
            discrete,
            coords,
            extrema,
            boxes: self.lines(),
            edges: self.lines(),
        }
    }
}

struct Snapshot {
    kind: VoxelShapeKind,
    is_block: bool,
    is_empty_singleton: bool,
    is_empty: bool,
    discrete: BitSetDiscreteVoxelShape,
    coords: [Coordinates; 3],
    extrema: [[f64; 2]; 3],
    boxes: Vec<[f64; 6]>,
    edges: Vec<[f64; 6]>,
}
impl Snapshot {
    fn imported(&self) -> VoxelShape {
        if self.is_block {
            block()
        } else if self.is_empty_singleton {
            empty()
        } else {
            VoxelShape::new(self.discrete.clone().into(), self.coords.clone(), self.kind)
        }
    }
    fn check(&self, shape: &VoxelShape, record: usize, tag: u8) {
        let label = format!("record {record}, tag {tag}");
        assert_eq!(shape.kind(), self.kind, "{label} kind");
        assert_eq!(
            shape.ptr_eq(&block()),
            self.is_block,
            "{label} block identity"
        );
        assert_eq!(
            shape.ptr_eq(&empty()),
            self.is_empty_singleton,
            "{label} empty identity"
        );
        assert_eq!(shape.is_empty(), self.is_empty, "{label} empty");
        for a in Axis::VALUES {
            assert_eq!(
                shape.discrete().size(a),
                self.discrete.size(a),
                "{label} size {a:?}"
            );
            assert_eq!(
                shape.discrete().first_full(a),
                self.discrete.first_full(a),
                "{label} first {a:?}"
            );
            assert_eq!(
                shape.discrete().last_full(a),
                self.discrete.last_full(a),
                "{label} last {a:?}"
            );
            assert_eq!(
                shape.coords(a).is_cube(),
                self.coords[a as usize].is_cube(),
                "{label} coordinate type {a:?}"
            );
            for i in 0..shape.coords(a).len() {
                assert_bits(
                    shape.coords(a).get(i),
                    self.coords[a as usize].get(i),
                    &label,
                );
            }
            assert_bits(shape.min(a), self.extrema[a as usize][0], &label);
            assert_bits(shape.max(a), self.extrema[a as usize][1], &label);
        }
        for x in 0..self.discrete.size(Axis::X) {
            for y in 0..self.discrete.size(Axis::Y) {
                for z in 0..self.discrete.size(Axis::Z) {
                    assert_eq!(
                        shape.discrete().is_full(x, y, z),
                        self.discrete.is_full(x, y, z),
                        "{label} cell {x},{y},{z}"
                    );
                }
            }
        }
        let mut boxes = Vec::new();
        shape.for_all_boxes(|line| boxes.push(line));
        check_lines(&boxes, &self.boxes, &label);
        let aabbs: Vec<_> = shape
            .to_aabbs()
            .into_iter()
            .map(|b| [b.min_x, b.min_y, b.min_z, b.max_x, b.max_y, b.max_z])
            .collect();
        check_lines(&aabbs, &self.boxes, &label);
        let mut edges = Vec::new();
        shape.for_all_edges(|line| edges.push(line));
        check_lines(&edges, &self.edges, &label);
        if !self.is_empty {
            let bounds = shape.bounds();
            for a in Axis::VALUES {
                assert_bits(bounds.min(a), self.extrema[a as usize][0], &label);
                assert_bits(bounds.max(a), self.extrema[a as usize][1], &label);
            }
        }
    }
}
fn assert_bits(actual: f64, expected: f64, label: &str) {
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "{label}: actual {actual:?}, vanilla {expected:?}"
    );
}
fn check_lines(actual: &[[f64; 6]], expected: &[[f64; 6]], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label} line count");
    for (a, e) in actual.iter().flatten().zip(expected.iter().flatten()) {
        assert_bits(*a, *e, label);
    }
}

/// Executes a graph of operations whose inputs and ordered expected results
/// were recorded by our Java harness from the unmodified vanilla 26.3 jar.
#[test]
fn vanilla_shape_operations_and_every_block_state() {
    let mut reader = Reader(include_bytes!("vanilla.bin"), Vec::new());
    assert_eq!(reader.int(), 0x53485031);
    let mut shapes = Vec::new();
    let mut record = 0;
    let mut states = 0;
    let mut rejected = [false; 16];
    let mut counts = [0; 16];
    loop {
        let tag = reader.byte();
        if tag == 255 {
            break;
        }
        record += 1;
        counts[tag as usize] += 1;
        let result = match tag {
            0 => {
                let expected = reader.snapshot();
                let shape = expected.imported();
                expected.check(&shape, record, tag);
                shapes.push(shape);
                None
            }
            1 => {
                let [x, y, z, u, v, w] = reader.doubles();
                Some(box_shape(x, y, z, u, v, w))
            }
            2 => {
                let a = reader.shape(&shapes);
                let b = reader.shape(&shapes);
                let op = BooleanOp::VALUES[reader.byte() as usize];
                let optimized = reader.boolean();
                let valid = reader.boolean();
                assert_eq!(valid, !op.apply(false, false));
                if valid {
                    assert_eq!(
                        join_is_not_empty(&a, &b, op),
                        reader.boolean(),
                        "record {record} join_is_not_empty {op:?}"
                    );
                    Some(if optimized {
                        join(&a, &b, op)
                    } else {
                        join_unoptimized(&a, &b, op)
                    })
                } else if !rejected[op as usize] {
                    assert!(std::panic::catch_unwind(|| join_unoptimized(&a, &b, op)).is_err());
                    assert!(std::panic::catch_unwind(|| join_is_not_empty(&a, &b, op)).is_err());
                    rejected[op as usize] = true;
                    None
                } else {
                    None
                }
            }
            3 => Some(reader.shape(&shapes).optimize()),
            4 => {
                let shape = reader.shape(&shapes);
                let [x, y, z] = reader.doubles();
                Some(shape.move_by(x, y, z))
            }
            5 => {
                let shape = reader.shape(&shapes);
                Some(shape.get_face_shape(Direction::VALUES[reader.byte() as usize]))
            }
            6 => {
                let shape = reader.shape(&shapes);
                let axis = Axis::VALUES[reader.byte() as usize];
                let moving = reader.aabb();
                let distance = reader.double();
                assert_bits(
                    shape.collide(axis, moving, distance),
                    reader.double(),
                    &format!("record {record} collide {axis:?} {moving:?} {distance:?}"),
                );
                None
            }
            7 => {
                let shape = reader.shape(&shapes);
                let from = reader.vec3();
                let to = reader.vec3();
                let pos = BlockPos::new(reader.int(), reader.int(), reader.int());
                let hit = shape.clip(from, to, pos);
                assert_eq!(
                    hit.is_some(),
                    reader.boolean(),
                    "record {record} clip {from:?} {to:?}"
                );
                if let Some(hit) = hit {
                    let expected = reader.doubles::<3>();
                    for (a, b) in [hit.location.x, hit.location.y, hit.location.z]
                        .into_iter()
                        .zip(expected)
                    {
                        assert_bits(a, b, &format!("record {record} clip"));
                    }
                    assert_eq!(
                        hit.direction,
                        Direction::VALUES[reader.byte() as usize],
                        "record {record} hit direction"
                    );
                    assert_eq!(hit.inside, reader.boolean(), "record {record} inside");
                    assert_eq!(hit.block_pos, pos);
                }
                None
            }
            8 => {
                let a = reader.shape(&shapes);
                let b = reader.shape(&shapes);
                let d = Direction::VALUES[reader.byte() as usize];
                assert_eq!(
                    block_occludes(&a, &b, d),
                    reader.boolean(),
                    "record {record} block_occludes {d:?}"
                );
                assert_eq!(
                    merged_face_occludes(&a, &b, d),
                    reader.boolean(),
                    "record {record} merged_face_occludes {d:?}"
                );
                assert_eq!(
                    face_shape_occludes(&a, &b),
                    reader.boolean(),
                    "record {record} face_shape_occludes"
                );
                assert_eq!(
                    is_face_full(&a, d),
                    reader.boolean(),
                    "record {record} is_face_full {d:?}"
                );
                None
            }
            9 => {
                let shape = reader.shape(&shapes);
                let permutation = std::array::from_fn(|_| Axis::VALUES[reader.byte() as usize]);
                let inversions = std::array::from_fn(|_| reader.boolean());
                Some(rotate_around(
                    &shape,
                    OctahedralGroup::new(permutation, inversions),
                    reader.vec3(),
                ))
            }
            10 => {
                assert_eq!(reader.count(), states, "registry state id");
                for _ in 0..3 {
                    reader.shape(&shapes);
                }
                states += 1;
                None
            }
            11 => {
                let shape = reader.shape(&shapes);
                for axis in Axis::VALUES {
                    for _ in 0..25 {
                        let [b, c, min, max] = reader.doubles();
                        assert_bits(
                            shape.min_at(axis, b, c),
                            min,
                            &format!("record {record} min_at"),
                        );
                        assert_bits(
                            shape.max_at(axis, b, c),
                            max,
                            &format!("record {record} max_at"),
                        );
                        assert_eq!(
                            shape.find_index(axis, b),
                            reader.int(),
                            "record {record} find_index {b:?}"
                        );
                    }
                }
                None
            }
            12 => {
                let a = reader.shape(&shapes);
                let b = reader.shape(&shapes);
                let axis = Axis::VALUES[reader.byte() as usize];
                let cost = reader.count();
                let first = reader.boolean();
                let second = reader.boolean();
                let merger = IndexMerger::new(cost, a.coords(axis), b.coords(axis), first, second);
                let expected_kind = [
                    IndexMergerKind::DiscreteCube,
                    IndexMergerKind::Identical,
                    IndexMergerKind::NonOverlapping,
                    IndexMergerKind::Indirect,
                ][reader.byte() as usize];
                assert_eq!(merger.kind(), expected_kind, "record {record} merger");
                assert_eq!(
                    merger.coordinates().len(),
                    reader.count(),
                    "record {record} merger size"
                );
                assert_eq!(
                    merger.coordinates().is_cube(),
                    reader.boolean(),
                    "record {record} merger coordinate type"
                );
                for i in 0..merger.coordinates().len() {
                    assert_bits(
                        merger.coordinates().get(i),
                        reader.double(),
                        &format!("record {record} merger coordinate"),
                    );
                }
                assert_eq!(
                    merger.indexes().len(),
                    reader.count(),
                    "record {record} merger indexes"
                );
                for (i, pair) in merger.indexes().iter().enumerate() {
                    assert_eq!(
                        *pair,
                        [reader.int(), reader.int()],
                        "record {record} mapping"
                    );
                    assert_eq!(i, reader.count(), "record {record} merged index");
                }
                None
            }
            13 => {
                let list: Vec<_> = (0..reader.count()).map(|_| reader.shape(&shapes)).collect();
                let axis = Axis::VALUES[reader.byte() as usize];
                let moving = reader.aabb();
                let distance = reader.double();
                assert_bits(
                    collide(axis, moving, &list, distance),
                    reader.double(),
                    &format!("record {record} iterable collision"),
                );
                None
            }
            14 => {
                let shape = reader.shape(&shapes);
                let kind = reader.byte();
                let descending = reader.boolean();
                let placement = reader.boolean();
                let y = reader.double();
                let fluid_collisions = reader.boolean();
                let default_value = reader.boolean();
                let context: Box<dyn CollisionContext<Item = u8, Fluid = u8>> = match kind {
                    0 => Box::new(EmptyCollisionContext::new(false)),
                    1 => Box::new(EmptyCollisionContext::new(true)),
                    2 => Box::new(PositionCollisionContext::new(y)),
                    3 => Box::new(EntityCollisionContext {
                        descending,
                        placement,
                        entity_bottom: y,
                        always_collide_with_fluid: fluid_collisions,
                        held_item: 1,
                        entity: NonLivingEntity,
                    }),
                    _ => panic!("unknown context kind"),
                };
                assert_eq!(
                    context.is_descending(),
                    reader.boolean(),
                    "record {record} descending"
                );
                assert_eq!(
                    context.is_placement(),
                    reader.boolean(),
                    "record {record} placement"
                );
                assert_eq!(
                    context.always_collide_with_fluid(),
                    reader.boolean(),
                    "record {record} fluid collisions"
                );
                assert_eq!(
                    context.is_above(&shape, BlockPos::new(0, 5, 0), default_value),
                    reader.boolean(),
                    "record {record} above"
                );
                assert_eq!(
                    context.is_holding_item(&1),
                    reader.boolean(),
                    "record {record} holding stick"
                );
                assert_eq!(
                    context.is_holding_item(&2),
                    reader.boolean(),
                    "record {record} holding stone"
                );
                assert_eq!(
                    context.can_stand_on_fluid(&0, &1),
                    reader.boolean(),
                    "record {record} stand on fluid"
                );
                None
            }
            15 => {
                let shape = reader.shape(&shapes);
                for _ in 0..4 {
                    let point = reader.vec3();
                    let closest = shape.closest_point_to(point);
                    assert_eq!(
                        closest.is_some(),
                        reader.boolean(),
                        "record {record} closest point presence"
                    );
                    if let Some(closest) = closest {
                        for actual in [closest.x, closest.y, closest.z] {
                            assert_bits(
                                actual,
                                reader.double(),
                                &format!("record {record} closest point"),
                            );
                        }
                    }
                }
                None
            }
            _ => panic!("unknown record tag {tag}"),
        };
        if let Some(shape) = result {
            reader.snapshot().check(&shape, record, tag);
            shapes.push(shape);
        }
    }
    assert!(reader.0.is_empty());
    assert_eq!(states, 35723);
    assert_eq!(shapes.len(), 15593);
    println!(
        "{record} vanilla records, {states} registry states, {} shapes; tag counts {counts:?}",
        shapes.len()
    );
}

struct NonLivingEntity;
impl EntityCollisionData for NonLivingEntity {
    type Item = u8;
    type Fluid = u8;
    type HeldItem = u8;
    fn is_holding_item(&self, held: &u8, item: &u8) -> bool {
        held == item
    }
    fn can_stand_on_fluid(&self, _fluid: &u8) -> bool {
        false
    }
    fn is_same_fluid(&self, a: &u8, b: &u8) -> bool {
        a == b
    }
}

#[test]
fn shared_shapes_are_send_sync_and_faces_do_not_retain_the_parent() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<VoxelShape>();
    let shape = block_box(0.0, 0.0, 0.0, 8.0, 8.0, 8.0);
    let cloned = shape.clone();
    assert!(shape.ptr_eq(&cloned));
    let face = shape.get_face_shape(Direction::Down);
    drop(shape);
    drop(cloned);
    assert_eq!(
        face.to_aabbs(),
        vec![Aabb::new(0.0, 0.0, 0.0, 0.5, 1.0, 0.5)]
    );
}

#[test]
fn iterable_collision_preserves_empty_and_tiny_distance_rules() {
    let bounds = Aabb::new(-1.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    assert_bits(
        collide(Axis::X, bounds, [], -0.0),
        -0.0,
        "no shapes preserves signed zero",
    );
    assert_bits(
        empty().collide(Axis::X, bounds, -0.0),
        -0.0,
        "empty shape preserves signed zero",
    );
    assert_bits(
        collide(Axis::X, bounds, [&empty()], -0.0),
        0.0,
        "iterable early return",
    );
}
