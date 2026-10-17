use super::*;
use std::{cmp::Ordering, fmt::Debug, rc::Rc, sync::OnceLock};

#[derive(Clone, Debug, PartialEq, Eq)]
struct StringKey(Vec<u16>);
impl JavaHash for StringKey {
    fn java_hash_code(&self) -> i32 {
        crate::java::string_hash_code_utf16(self.0.iter().copied())
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp(&other.0))
    }
}
#[derive(Debug)]
struct Step {
    op: u8,
    key: usize,
    value: u8,
    capacity: usize,
    order: Vec<(u8, u8)>,
}
struct Case {
    name: String,
    keys: String,
    f: f32,
    steps: Vec<Step>,
}
fn cases() -> &'static [Case] {
    static CASES: OnceLock<Vec<Case>> = OnceLock::new();
    CASES.get_or_init(|| {
        let data = include_bytes!("oracle.bin");
        assert_eq!(&data[..4], b"FLC1");
        let mut pos = 4;
        let mut cases = Vec::<Case>::new();
        while pos < data.len() {
            if data[pos] == 255 {
                let len = data[pos + 1] as usize;
                pos += 2;
                let header = std::str::from_utf8(&data[pos..pos + len]).unwrap();
                pos += len;
                let fields = header.split_whitespace().collect::<Vec<_>>();
                cases.push(Case {
                    name: fields[0].into(),
                    keys: fields[1].into(),
                    f: fields[3].parse().unwrap(),
                    steps: Vec::new(),
                });
            } else {
                let op = data[pos];
                let key = data[pos + 1] as usize;
                let value = data[pos + 2];
                let capacity = u16::from_le_bytes([data[pos + 3], data[pos + 4]]) as usize;
                let prefix = data[pos + 5] as usize;
                let removed = data[pos + 6] as usize;
                let inserted = data[pos + 7] as usize;
                pos += 8;
                let case = cases.last_mut().unwrap();
                let mut order = case.steps.last().map_or_else(Vec::new, |s| s.order.clone());
                order.splice(
                    prefix..prefix + removed,
                    (0..inserted).map(|i| (data[pos + i * 2], data[pos + i * 2 + 1])),
                );
                pos += inserted * 2;
                case.steps.push(Step {
                    op,
                    key,
                    value,
                    capacity,
                    order,
                });
            }
        }
        assert_eq!(cases.len(), 180);
        for c in &cases {
            assert_eq!(c.steps.len(), 720);
        }
        cases
    })
}
fn long_keys() -> Vec<i64> {
    let mut keys = vec![0, -1, i64::MIN, i64::MAX];
    for n in -100000_i64.. {
        if mix_long(n) & 255 == 255 {
            keys.push(n);
        }
        if keys.len() == 160 {
            break;
        }
    }
    keys
}
fn int_keys() -> Vec<i32> {
    let mut keys = vec![0, -1, i32::MIN, i32::MAX];
    for n in -100000_i32.. {
        if mix_int(n) & 255 == 255 {
            keys.push(n);
        }
        if keys.len() == 160 {
            break;
        }
    }
    keys
}
fn string_keys() -> Vec<Option<StringKey>> {
    (0..160)
        .map(|id| match id {
            0 => None,
            1 => Some(StringKey(vec![0xd83d, 0xde00, 0xd800])),
            _ => Some(StringKey(
                (0..8)
                    .flat_map(|bit| {
                        if id & (1 << bit) == 0 {
                            [65, 97]
                        } else {
                            [66, 66]
                        }
                    })
                    .collect(),
            )),
        })
        .collect()
}
trait Value: Debug + PartialEq {
    fn from_id(id: u8) -> Self;
    fn to_id(&self) -> u8;
}
macro_rules! value_number { ($($ty:ty),*)=>{$(impl Value for $ty {
    fn from_id(id:u8)->Self {id as Self}
    fn to_id(&self)->u8 {*self as u8}
})*}; }
value_number!(i8, i32, i64, f32, f64);
impl Value for String {
    fn from_id(id: u8) -> Self {
        format!("v{id}")
    }
    fn to_id(&self) -> u8 {
        self[1..].parse().unwrap()
    }
}
impl Value for bool {
    fn from_id(id: u8) -> Self {
        id & 1 != 0
    }
    fn to_id(&self) -> u8 {
        u8::from(*self)
    }
}
fn assert_order<K: Clone + Debug + Eq>(
    actual: impl Iterator<Item = (K, u8)>,
    keys: &[K],
    step: &Step,
    context: &str,
) {
    let expected = step
        .order
        .iter()
        .map(|&(k, v)| (keys[k as usize].clone(), v))
        .collect::<Vec<_>>();
    assert_eq!(actual.collect::<Vec<_>>(), expected, "{context}");
}
fn run_open_map<K: Clone + Debug + Eq, V: Value, H: OpenHashKey<K>, const L: bool>(
    name: &str,
    keys: &[K],
) {
    for case in cases().iter().filter(|c| c.name == name) {
        let mut map = OpenHashMap::<K, V, H, L>::with_capacity_and_load_factor(0, case.f);
        for (i, step) in case.steps.iter().enumerate() {
            let key = keys[step.key].clone();
            let value = || V::from_id(step.value);
            match step.op {
                0 => {
                    map.insert(key, value());
                }
                1 => {
                    map.remove(&key);
                }
                2 => map.trim(),
                3 => map.clear(),
                4 | 5 => {
                    map.put_for_test(key, value(), step.op == 4);
                }
                6 | 7 => {
                    map.move_for_test(&key, step.op == 6);
                }
                8 | 9 => {
                    map.pop_for_test(step.op == 8);
                }
                10 => {
                    let mut cursor = map.cursor();
                    let mut count = 0;
                    while cursor.next_entry().is_some() {
                        if count % 3 == step.key % 3 {
                            cursor.remove();
                        }
                        count += 1;
                    }
                }
                11 => map.trim_to(step.key),
                _ => panic!("bad op"),
            }
            let context = format!(
                "{name} f={} step={i} op={} key={}",
                case.f, step.op, step.key
            );
            assert_eq!(map.capacity(), step.capacity, "capacity {context}");
            assert_eq!(map.len(), step.order.len(), "size {context}");
            assert_order::<K>(
                map.iter().map(|(k, v)| (k.clone(), v.to_id())),
                keys,
                step,
                &context,
            );
            let reverse = map
                .iter()
                .rev()
                .map(|(k, v)| (k.clone(), v.to_id()))
                .collect::<Vec<_>>();
            assert_eq!(
                reverse,
                map.iter()
                    .map(|(k, v)| (k.clone(), v.to_id()))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>(),
                "reverse {context}"
            );
            for (k, v) in &map {
                assert_eq!(map.get(k), Some(v));
            }
        }
        let expected = map
            .iter()
            .map(|(k, v)| (k.clone(), v.to_id()))
            .collect::<Vec<_>>();
        assert_eq!(
            map.into_iter()
                .map(|(k, v)| (k, v.to_id()))
                .collect::<Vec<_>>(),
            expected
        );
    }
}
// These helpers exercise only operations available for linked Java variants.
// Keeping access to the engine here avoids lying to the type system with casts.
impl<K: Eq, V, H: OpenHashKey<K>, const L: bool> OpenHashMap<K, V, H, L> {
    fn put_for_test(&mut self, key: K, value: V, first: bool) {
        assert!(L);
        self.put(key, value, Some(first));
    }
    fn move_for_test(&mut self, key: &K, first: bool) {
        assert!(L);
        let pos = self.slot(key);
        if self.table[pos].is_some() {
            self.move_index(pos, first);
        }
    }
    fn pop_for_test(&mut self, first: bool) {
        assert!(L);
        if let Some(pos) = if first { self.first } else { self.last } {
            self.remove_index(pos, true);
        }
    }
}
fn run_open_set<K: Clone + Debug + Eq, H: OpenHashKey<K>, const L: bool>(name: &str, keys: &[K]) {
    for case in cases().iter().filter(|c| c.name == name) {
        let mut set = OpenHashSet::<K, H, L>::with_capacity_and_load_factor(0, case.f);
        for (i, step) in case.steps.iter().enumerate() {
            let key = keys[step.key].clone();
            match step.op {
                0 => {
                    set.insert(key);
                }
                1 => {
                    set.remove(&key);
                }
                2 => set.trim(),
                3 => set.clear(),
                4 | 5 => set.0.put_for_test(key, (), step.op == 4),
                6 | 7 => {}
                8 | 9 => set.0.pop_for_test(step.op == 8),
                10 => {
                    let mut cursor = set.cursor();
                    let mut count = 0;
                    while cursor.next_entry().is_some() {
                        if count % 3 == step.key % 3 {
                            cursor.remove();
                        }
                        count += 1;
                    }
                }
                11 => set.trim_to(step.key),
                _ => panic!("bad op"),
            }
            let context = format!("{name} f={} step={i}", case.f);
            assert_eq!(set.capacity(), step.capacity, "capacity {context}");
            assert_eq!(set.len(), step.order.len());
            assert_order::<K>(set.iter().map(|k| (k.clone(), 0)), keys, step, &context);
        }
        let expected = set.iter().cloned().collect::<Vec<_>>();
        assert_eq!(set.into_iter().collect::<Vec<_>>(), expected);
    }
}
macro_rules! open_map_test {
    ($test:ident,$name:literal,$key:ty,$value:ty,$hash:ty,$linked:literal,$keys:expr) => {
        #[test]
        fn $test() {
            run_open_map::<$key, $value, $hash, $linked>($name, &$keys);
        }
    };
}
macro_rules! open_set_test {
    ($test:ident,$name:literal,$key:ty,$hash:ty,$linked:literal,$keys:expr) => {
        #[test]
        fn $test() {
            run_open_set::<$key, $hash, $linked>($name, &$keys);
        }
    };
}
open_set_test!(
    long_set,
    "LongOpenHashSet",
    i64,
    LongHash,
    false,
    long_keys()
);
open_set_test!(
    long_linked_set,
    "LongLinkedOpenHashSet",
    i64,
    LongHash,
    true,
    long_keys()
);
open_set_test!(int_set, "IntOpenHashSet", i32, IntHash, false, int_keys());
open_set_test!(
    object_set,
    "ObjectOpenHashSet",
    Option<StringKey>,
    ObjectHash,
    false,
    string_keys()
);
open_set_test!(
    object_linked_set,
    "ObjectLinkedOpenHashSet",
    Option<StringKey>,
    ObjectHash,
    true,
    string_keys()
);
open_set_test!(
    short_set,
    "ShortOpenHashSet",
    i16,
    ShortHash,
    false,
    int_keys().into_iter().map(|k| k as i16).collect::<Vec<_>>()
);
open_set_test!(
    char_set,
    "CharOpenHashSet",
    u16,
    CharHash,
    false,
    int_keys().into_iter().map(|k| k as u16).collect::<Vec<_>>()
);
open_map_test!(
    long_object_map,
    "Long2ObjectOpenHashMap",
    i64,
    String,
    LongHash,
    false,
    long_keys()
);
open_map_test!(
    long_object_linked_map,
    "Long2ObjectLinkedOpenHashMap",
    i64,
    String,
    LongHash,
    true,
    long_keys()
);
open_map_test!(
    long_int_map,
    "Long2IntOpenHashMap",
    i64,
    i32,
    LongHash,
    false,
    long_keys()
);
open_map_test!(
    long_long_map,
    "Long2LongOpenHashMap",
    i64,
    i64,
    LongHash,
    false,
    long_keys()
);
open_map_test!(
    long_byte_map,
    "Long2ByteOpenHashMap",
    i64,
    i8,
    LongHash,
    false,
    long_keys()
);
open_map_test!(
    int_object_map,
    "Int2ObjectOpenHashMap",
    i32,
    String,
    IntHash,
    false,
    int_keys()
);
open_map_test!(
    object_object_map,
    "Object2ObjectOpenHashMap",
    Option<StringKey>,
    String,
    ObjectHash,
    false,
    string_keys()
);
open_map_test!(
    object_int_map,
    "Object2IntOpenHashMap",
    Option<StringKey>,
    i32,
    ObjectHash,
    false,
    string_keys()
);
open_map_test!(
    object_object_linked_map,
    "Object2ObjectLinkedOpenHashMap",
    Option<StringKey>,
    String,
    ObjectHash,
    true,
    string_keys()
);
open_map_test!(
    object_boolean_map,
    "Object2BooleanOpenHashMap",
    Option<StringKey>,
    bool,
    ObjectHash,
    false,
    string_keys()
);
open_map_test!(
    object_byte_linked_map,
    "Object2ByteLinkedOpenHashMap",
    Option<StringKey>,
    i8,
    ObjectHash,
    true,
    string_keys()
);
open_map_test!(
    int_object_linked_map,
    "Int2ObjectLinkedOpenHashMap",
    i32,
    String,
    IntHash,
    true,
    int_keys()
);
open_map_test!(
    object_long_map,
    "Object2LongOpenHashMap",
    Option<StringKey>,
    i64,
    ObjectHash,
    false,
    string_keys()
);
open_map_test!(
    object_int_linked_map,
    "Object2IntLinkedOpenHashMap",
    Option<StringKey>,
    i32,
    ObjectHash,
    true,
    string_keys()
);
open_map_test!(
    int_long_map,
    "Int2LongOpenHashMap",
    i32,
    i64,
    IntHash,
    false,
    int_keys()
);
open_map_test!(
    short_object_map,
    "Short2ObjectOpenHashMap",
    i16,
    String,
    ShortHash,
    false,
    int_keys().into_iter().map(|k| k as i16).collect::<Vec<_>>()
);
open_map_test!(
    int_int_map,
    "Int2IntOpenHashMap",
    i32,
    i32,
    IntHash,
    false,
    int_keys()
);
open_map_test!(
    long_float_linked_map,
    "Long2FloatLinkedOpenHashMap",
    i64,
    f32,
    LongHash,
    true,
    long_keys()
);
open_map_test!(
    short_boolean_map,
    "Short2BooleanOpenHashMap",
    i16,
    bool,
    ShortHash,
    false,
    int_keys().into_iter().map(|k| k as i16).collect::<Vec<_>>()
);
open_map_test!(
    long_long_linked_map,
    "Long2LongLinkedOpenHashMap",
    i64,
    i64,
    LongHash,
    true,
    long_keys()
);
open_map_test!(
    long_boolean_map,
    "Long2BooleanOpenHashMap",
    i64,
    bool,
    LongHash,
    false,
    long_keys()
);
open_map_test!(
    int_int_linked_map,
    "Int2IntLinkedOpenHashMap",
    i32,
    i32,
    IntHash,
    true,
    int_keys()
);
open_map_test!(
    int_double_map,
    "Int2DoubleOpenHashMap",
    i32,
    f64,
    IntHash,
    false,
    int_keys()
);

fn run_java_map<const L: bool>(name: &str) {
    run_java_map_keys::<_, L>(name, "string", string_keys());
    run_java_map_keys::<_, L>(name, "long", long_keys());
    run_java_map_keys::<_, L>(name, "int", int_keys());
    run_java_map_keys::<_, L>(name, "tree", (0..160).map(TreeKey).collect());
    run_java_map_keys::<_, L>(
        name,
        "blockpos",
        (0..160)
            .map(|id| crate::BlockPos::new((id - 80) * 64, 0, 0))
            .collect(),
    );
    run_java_map_keys::<_, L>(
        name,
        "vec3i",
        (0..160)
            .map(|id| crate::Vec3i::new(-31 * id, id, 0))
            .collect(),
    );
}
fn run_java_map_keys<K: JavaHash + Eq + Clone + Debug, const L: bool>(
    name: &str,
    kind: &str,
    keys: Vec<K>,
) {
    for case in cases().iter().filter(|c| c.name == name && c.keys == kind) {
        let mut map = JavaHashMap::<K, String, L>::with_capacity_and_load_factor(0, case.f);
        map.access_order = name == "JavaAccessHashMap";
        for (i, step) in case.steps.iter().enumerate() {
            let key = keys[step.key].clone();
            match step.op {
                0 => {
                    map.insert(key, String::from_id(step.value));
                }
                1 => {
                    map.remove(&key);
                }
                3 => map.clear(),
                4 | 5 => map.reposition_for_test(key, String::from_id(step.value), step.op == 4),
                6 | 7 | 12 => {
                    map.get(&key);
                }
                8 | 9 => map.pop_for_test(step.op == 8),
                10 => {
                    let mut c = map.cursor();
                    let mut count = 0;
                    while c.next_entry().is_some() {
                        if count % 3 == step.key % 3 {
                            c.remove();
                        }
                        count += 1;
                    }
                }
                _ => panic!("bad java op"),
            }
            let context = format!("{name} f={} step={i} op={}", case.f, step.op);
            assert_eq!(map.capacity(), step.capacity, "capacity {context}");
            assert_eq!(map.len(), step.order.len());
            assert_order::<_>(
                map.iter().map(|(k, v)| (k.clone(), v.to_id())),
                &keys,
                step,
                &context,
            );
            for (k, v) in &map {
                assert_eq!(map.peek(k), Some(v), "lookup {context}");
            }
        }
        let expected = map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        assert_eq!(map.into_iter().collect::<Vec<_>>(), expected);
    }
}
impl<K: Eq + JavaHash, V, const L: bool> JavaHashMap<K, V, L> {
    fn reposition_for_test(&mut self, key: K, value: V, first: bool) {
        assert!(L);
        let existing = self.find(&key);
        self.insert(key, value);
        let id = existing.or(self.last).unwrap();
        self.unlink(id);
        if first {
            self.n_mut(id).prev = None;
            self.n_mut(id).next = self.first;
            if let Some(first) = self.first {
                self.n_mut(first).prev = Some(id);
            } else {
                self.last = Some(id);
            }
            self.first = Some(id);
        } else {
            self.link_last(id);
        }
    }
    fn pop_for_test(&mut self, first: bool) {
        assert!(L);
        if let Some(id) = if first { self.first } else { self.last } {
            self.remove_id(id, true);
        }
    }
}
fn run_java_set<const L: bool>(name: &str) {
    run_java_set_keys::<_, L>(name, "string", string_keys());
    run_java_set_keys::<_, L>(name, "long", long_keys());
    run_java_set_keys::<_, L>(name, "int", int_keys());
    run_java_set_keys::<_, L>(name, "tree", (0..160).map(TreeKey).collect());
    run_java_set_keys::<_, L>(
        name,
        "blockpos",
        (0..160)
            .map(|id| crate::BlockPos::new((id - 80) * 64, 0, 0))
            .collect(),
    );
    run_java_set_keys::<_, L>(
        name,
        "vec3i",
        (0..160)
            .map(|id| crate::Vec3i::new(-31 * id, id, 0))
            .collect(),
    );
}
fn run_java_set_keys<K: JavaHash + Eq + Clone + Debug, const L: bool>(
    name: &str,
    kind: &str,
    keys: Vec<K>,
) {
    for case in cases().iter().filter(|c| c.name == name && c.keys == kind) {
        let mut set = JavaHashSet::<K, L>::with_capacity_and_load_factor(0, case.f);
        for (i, step) in case.steps.iter().enumerate() {
            let key = keys[step.key].clone();
            match step.op {
                0 => {
                    set.insert(key);
                }
                1 => {
                    set.remove(&key);
                }
                3 => set.clear(),
                4 | 5 => set.0.reposition_for_test(key, (), step.op == 4),
                6 | 7 | 12 => {}
                8 | 9 => set.0.pop_for_test(step.op == 8),
                10 => {
                    let mut c = set.cursor();
                    let mut count = 0;
                    while c.next_entry().is_some() {
                        if count % 3 == step.key % 3 {
                            c.remove();
                        }
                        count += 1;
                    }
                }
                _ => panic!("bad java op"),
            }
            let context = format!("{name} f={} step={i}", case.f);
            assert_eq!(set.capacity(), step.capacity, "capacity {context}");
            assert_eq!(set.len(), step.order.len());
            assert_order::<_>(set.iter().map(|k| (k.clone(), 0)), &keys, step, &context);
            for k in &set {
                assert!(set.contains(k));
            }
        }
    }
}
#[test]
fn jdk_hash_map() {
    run_java_map::<false>("JavaHashMap");
}
#[test]
fn jdk_linked_hash_map() {
    run_java_map::<true>("JavaLinkedHashMap");
}
#[test]
fn jdk_access_hash_map() {
    run_java_map::<true>("JavaAccessHashMap");
}
#[test]
fn jdk_hash_set() {
    run_java_set::<false>("JavaHashSet");
}
#[test]
fn jdk_linked_hash_set() {
    run_java_set::<true>("JavaLinkedHashSet");
}

#[test]
fn reference_identity_and_deterministic_order() {
    let a = Rc::new("same".to_owned());
    let b = Rc::new("same".to_owned());
    let mut set = ReferenceOpenHashSet::new();
    assert!(set.insert(a.clone()));
    assert!(set.insert(b.clone()));
    assert!(!set.insert(a.clone()));
    assert_eq!(set.len(), 2);
    assert!(Rc::ptr_eq(set.first().unwrap(), &a));
    assert!(Rc::ptr_eq(set.last().unwrap(), &b));
    assert!(set.remove(&a));
    set.insert(a.clone());
    assert!(Rc::ptr_eq(set.first().unwrap(), &b));
    let mut map = Reference2IntOpenHashMap::new();
    map.insert(a.clone(), 1);
    map.insert(b.clone(), 2);
    map.trim();
    assert_eq!(map.get(&a), Some(&1));
    assert_eq!(map.get(&b), Some(&2));
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [1, 2]);
    map.insert_and_move_to_first(b.clone(), 3);
    assert!(Rc::ptr_eq(map.first().unwrap().0, &b));
    assert_eq!(map.into_iter().map(|(_, v)| v).collect::<Vec<_>>(), [3, 1]);
}
#[test]
fn entry_helpers_and_linked_endpoints() {
    let mut m = Long2ObjectLinkedOpenHashMap::new();
    m.get_or_insert_with(1, || "one".to_owned());
    m.get_or_insert_with(1, || panic!("occupied"));
    m.insert_and_move_to_first(2, "two".into());
    assert_eq!(m.first().map(|(k, _)| *k), Some(2));
    assert_eq!(m.last().map(|(k, _)| *k), Some(1));
    m.get_and_move_to_first(&1);
    assert_eq!(m.first().map(|(k, _)| *k), Some(1));
    m.get_and_move_to_last(&1);
    assert_eq!(m.pop_first().map(|(k, _)| k), Some(2));
    assert_eq!(m.pop_last().map(|(k, _)| k), Some(1));
    let mut j = LinkedHashMap::with_access_order(0, 0.75, true);
    j.insert(1, 1);
    j.insert(2, 2);
    j.insert_first(2, 3);
    assert_eq!(j.keys().copied().collect::<Vec<_>>(), [2, 1]);
    j.insert_last(2, 4);
    assert_eq!(j.keys().copied().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(j.pop_last(), Some((2, 4)));
    assert_eq!(j.pop_first(), Some((1, 1)));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeKey(i32);
impl JavaHash for TreeKey {
    fn java_hash_code(&self) -> i32 {
        (self.0 - 80) * 64
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

#[test]
fn java_hashes_match_vanilla_util_and_jdk() {
    let strings = ["", "abc", "Aa", "BB", "😀", "\u{ffff}foo"];
    let doubles = [
        0.0,
        -0.0,
        1.25,
        -7.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for line in include_str!("hashes.txt").lines() {
        let f = line.split_whitespace().collect::<Vec<_>>();
        let at = |i: usize| f[i].parse::<i32>().unwrap();
        match f[0] {
            "string" => {
                let i = f[1].parse::<usize>().unwrap();
                let actual = if i == 4 {
                    crate::java::string_hash_code_utf16([0xd83d, 0xde00, 0xd800])
                } else {
                    strings[i].java_hash_code()
                };
                assert_eq!(actual, at(2));
            }
            "long" => {
                let n = f[1].parse::<i64>().unwrap();
                assert_eq!(n.java_hash_code(), at(2));
            }
            "uuid" => {
                let n = f[1].parse::<i64>().unwrap();
                assert_eq!(
                    crate::Uuid::new(n, n.rotate_left(17)).java_hash_code(),
                    at(2)
                );
            }
            "pos" => {
                let x = at(1);
                let y = x.wrapping_mul(31);
                let z = x ^ (-559038737);
                assert_eq!(JavaHash::java_hash_code(&crate::Vec3i::new(x, y, z)), at(2));
                assert_eq!(
                    JavaHash::java_hash_code(&crate::BlockPos::new(x, y, z)),
                    at(3)
                );
                assert_eq!(
                    JavaHash::java_hash_code(&crate::SectionPos::new(x, y, z)),
                    at(4)
                );
                assert_eq!(JavaHash::java_hash_code(&crate::ChunkPos::new(x, z)), at(5));
            }
            "record" => assert_eq!(record_hash_code([at(1), "abc".java_hash_code()]), at(2)),
            "geometry" => {
                let i = f[1].parse::<usize>().unwrap();
                let (x, y, z) = (doubles[i], doubles[(i + 1) % 7], doubles[(i + 2) % 7]);
                assert_eq!(crate::Vec3::new(x, y, z).java_hash_code(), at(2));
                assert_eq!(crate::Aabb::new(x, y, z, z, x, y).java_hash_code(), at(3));
            }
            "identifier" => assert_eq!(identifier_hash_code("minecraft", "stone"), at(1)),
            "boolean" => {
                assert_eq!(true.java_hash_code(), at(1));
                assert_eq!(false.java_hash_code(), at(2));
            }
            other => panic!("unknown oracle kind {other}"),
        }
    }
    // A subclass inheriting Comparable<Vec3i> is not screened as Comparable<BlockPos>.
    assert_eq!(
        JavaHash::java_compare(&crate::BlockPos::ZERO, &crate::BlockPos::ZERO),
        None
    );
    assert_eq!(
        JavaHash::java_compare(&crate::Vec3i::ZERO, &crate::Vec3i::new(0, 1, 0)),
        Some(Ordering::Less)
    );
}
