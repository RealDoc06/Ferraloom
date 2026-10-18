use super::*;
use ferraloom_util::{Aabb, Axis};

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
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
    fn count(&mut self) -> usize {
        u32::from_be_bytes(self.bytes()) as usize
    }
    fn int(&mut self) -> i32 {
        i32::from_be_bytes(self.bytes())
    }
    fn short(&mut self) -> u16 {
        u16::from_be_bytes(self.bytes())
    }
    fn bits(&mut self) -> u64 {
        u64::from_be_bytes(self.bytes())
    }
    fn lines(&mut self) -> Vec<[u64; 6]> {
        (0..self.count())
            .map(|_| std::array::from_fn(|_| self.bits()))
            .collect()
    }
    fn snapshot(&mut self) -> Snapshot {
        let kind = [
            VoxelShapeKind::Cube,
            VoxelShapeKind::Array,
            VoxelShapeKind::Slice,
        ][self.byte() as usize];
        let block = self.boolean();
        let empty_singleton = self.boolean();
        let empty = self.boolean();
        let mut size = [0; 3];
        let mut first = [0; 3];
        let mut last = [0; 3];
        let mut cube = [false; 3];
        let mut extrema = [[0; 2]; 3];
        let coordinates = std::array::from_fn(|a| {
            size[a] = self.int();
            first[a] = self.int();
            last[a] = self.int();
            cube[a] = self.boolean();
            let coordinates = (0..=size[a]).map(|_| self.bits()).collect();
            extrema[a] = [self.bits(), self.bits()];
            coordinates
        });
        let occupancy = (0..size[0] * size[1] * size[2])
            .map(|_| self.boolean())
            .collect();
        Snapshot {
            kind,
            block,
            empty_singleton,
            empty,
            size,
            first,
            last,
            cube,
            coordinates,
            extrema,
            occupancy,
            boxes: self.lines(),
            edges: self.lines(),
        }
    }
}

struct Snapshot {
    kind: VoxelShapeKind,
    block: bool,
    empty_singleton: bool,
    empty: bool,
    size: [i32; 3],
    first: [i32; 3],
    last: [i32; 3],
    cube: [bool; 3],
    coordinates: [Vec<u64>; 3],
    extrema: [[u64; 2]; 3],
    occupancy: Vec<bool>,
    boxes: Vec<[u64; 6]>,
    edges: Vec<[u64; 6]>,
}
impl Snapshot {
    fn check(&self, shape: &VoxelShape) {
        assert_eq!(shape.kind(), self.kind);
        assert_eq!(shape.ptr_eq(&shapes::block()), self.block);
        assert_eq!(shape.ptr_eq(&shapes::empty()), self.empty_singleton);
        assert_eq!(shape.is_empty(), self.empty);
        for (i, axis) in Axis::VALUES.into_iter().enumerate() {
            assert_eq!(shape.discrete().size(axis), self.size[i]);
            assert_eq!(shape.discrete().first_full(axis), self.first[i]);
            assert_eq!(shape.discrete().last_full(axis), self.last[i]);
            assert_eq!(shape.coords(axis).is_cube(), self.cube[i]);
            let actual: Vec<_> = (0..shape.coords(axis).len())
                .map(|j| shape.coords(axis).get(j).to_bits())
                .collect();
            assert_eq!(actual, self.coordinates[i]);
            assert_eq!(
                [shape.min(axis).to_bits(), shape.max(axis).to_bits()],
                self.extrema[i]
            );
        }
        let mut index = 0;
        for x in 0..self.size[0] {
            for y in 0..self.size[1] {
                for z in 0..self.size[2] {
                    assert_eq!(shape.discrete().is_full(x, y, z), self.occupancy[index]);
                    index += 1;
                }
            }
        }
        let mut boxes = Vec::new();
        shape.for_all_boxes(|line| boxes.push(line.map(f64::to_bits)));
        assert_eq!(boxes, self.boxes);
        let mut edges = Vec::new();
        shape.for_all_edges(|line| edges.push(line.map(f64::to_bits)));
        assert_eq!(edges, self.edges);
        let aabbs: Vec<_> = shape.to_aabbs().into_iter().map(aabb_bits).collect();
        assert_eq!(aabbs, self.boxes);
    }
}
fn aabb_bits(aabb: Aabb) -> [u64; 6] {
    [
        aabb.min_x, aabb.min_y, aabb.min_z, aabb.max_x, aabb.max_y, aabb.max_z,
    ]
    .map(f64::to_bits)
}

/// Independent Java harness output, not the JSON consumed by the generator.
/// Checks all nine cache entries for every state, including dynamic baselines.
#[test]
fn every_native_state_cache_matches_vanilla_snapshot_bits_and_identity() {
    let mut reader = Reader(include_bytes!("../../tests/vanilla_voxel_shapes.bin"));
    assert_eq!(reader.count(), 0x53484e31);
    let count = reader.count();
    assert_eq!(count, NATIVE_SHAPES.len());
    let mut snapshots = Vec::new();
    let mut instance_snapshots = Vec::new();
    for _ in 0..count {
        let index = reader.count();
        if index == snapshots.len() {
            let length = reader.count();
            let (bytes, rest) = reader.0.split_at(length);
            reader.0 = rest;
            let mut input = Reader(bytes);
            snapshots.push(input.snapshot());
            assert!(input.0.is_empty());
        }
        instance_snapshots.push(index);
    }
    let states = reader.count();
    assert_eq!(states, STATE_SHAPES.len());
    assert_eq!(states, 35723);
    assert_eq!(reader.byte(), 9);
    let mut checked = vec![false; count];
    let mut identities = vec![None; count];
    let mut inverse_identities = vec![None; count];
    for id in 0..states {
        let state = BlockState::from_id(id as u16).unwrap();
        let cached = state.cached_shapes();
        let context = reader.boolean();
        assert_eq!(cached.context_dependent, context);
        assert_eq!(state.data().context_dependent_shape, context);
        let ids = [
            cached.collision,
            cached.outline,
            cached.occlusion,
            cached.face_occlusion[0],
            cached.face_occlusion[1],
            cached.face_occlusion[2],
            cached.face_occlusion[3],
            cached.face_occlusion[4],
            cached.face_occlusion[5],
        ];
        for native in ids {
            let expected = reader.short() as usize;
            let index = native.0 as usize;
            if let Some(previous) = identities[expected] {
                assert_eq!(index, previous, "state {id} shared vanilla identity");
            } else {
                identities[expected] = Some(index);
            }
            if let Some(previous) = inverse_identities[index] {
                assert_eq!(expected, previous, "state {id} distinct vanilla identity");
            } else {
                inverse_identities[index] = Some(expected);
            }
            if !checked[index] {
                snapshots[instance_snapshots[expected]].check(native.get());
                checked[index] = true;
            }
            assert!(
                std::ptr::eq(native.get(), native.get()),
                "shape cache is stable"
            );
        }
        if context {
            assert!(state.collision_shape().is_none());
            assert!(state.shape().is_none());
        } else {
            assert!(
                state
                    .collision_shape()
                    .unwrap()
                    .ptr_eq(cached.collision.get())
            );
            assert!(state.shape().unwrap().ptr_eq(cached.outline.get()));
        }
        assert!(state.occlusion_shape().ptr_eq(cached.occlusion.get()));
        for direction in Direction::VALUES {
            assert!(
                state
                    .face_occlusion_shape(direction)
                    .ptr_eq(cached.face_occlusion[direction as usize].get())
            );
        }
    }
    assert!(checked.into_iter().all(|checked| checked));
    assert!(reader.0.is_empty());
}
