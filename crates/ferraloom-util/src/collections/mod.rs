//! Vanilla collection traversal order is observable simulation behavior.
//!
//! Value-key collections reproduce fastutil 8.5.18 and JDK 25 table traversal,
//! resizing and deletion. Primitive fastutil keys use different mixing from
//! boxed/object keys, so choose the corresponding named alias.
//!
//! Identity-key collections deliberately use deterministic insertion order:
//! vanilla's identity hashes (including enums and ResourceKey) vary across runs.
//! Implement JavaHash for other crates' value types; never use Rust Hash output
//! or enum ordinals as a substitute for Java hashCode.
mod hash;
mod jdk;
mod open;
mod reference;
pub use hash::*;
pub use jdk::{JavaCursor, JavaHashMap, JavaHashSet, JavaIter};
pub use open::{Cursor, Iter, OpenHashMap, OpenHashSet};
pub use reference::{ReferenceHashMap, ReferenceIter, ReferenceKey, ReferenceOpenHashSet};

pub type LongOpenHashSet = OpenHashSet<i64, LongHash>;
pub type LongLinkedOpenHashSet = OpenHashSet<i64, LongHash, true>;
pub type IntOpenHashSet = OpenHashSet<i32, IntHash>;
pub type ObjectOpenHashSet<K> = OpenHashSet<K>;
pub type ObjectLinkedOpenHashSet<K> = OpenHashSet<K, ObjectHash, true>;
pub type Long2ObjectOpenHashMap<V> = OpenHashMap<i64, V, LongHash>;
pub type Long2ObjectLinkedOpenHashMap<V> = OpenHashMap<i64, V, LongHash, true>;
pub type Long2IntOpenHashMap = OpenHashMap<i64, i32, LongHash>;
pub type Long2LongOpenHashMap = OpenHashMap<i64, i64, LongHash>;
pub type Long2ByteOpenHashMap = OpenHashMap<i64, i8, LongHash>;
pub type Int2ObjectOpenHashMap<V> = OpenHashMap<i32, V, IntHash>;
pub type Object2ObjectOpenHashMap<K, V> = OpenHashMap<K, V>;
pub type Object2IntOpenHashMap<K> = OpenHashMap<K, i32>;
pub type Object2ObjectLinkedOpenHashMap<K, V> = OpenHashMap<K, V, ObjectHash, true>;
// Further variants found in vanilla, in descending usage frequency.
pub type Reference2ObjectOpenHashMap<K, V> = ReferenceHashMap<K, V>;
pub type Reference2IntOpenHashMap<K> = ReferenceHashMap<K, i32>;
pub type Object2BooleanOpenHashMap<K> = OpenHashMap<K, bool>;
pub type Object2ByteLinkedOpenHashMap<K> = OpenHashMap<K, i8, ObjectHash, true>;
pub type Int2ObjectLinkedOpenHashMap<V> = OpenHashMap<i32, V, IntHash, true>;
pub type Object2LongOpenHashMap<K> = OpenHashMap<K, i64>;
pub type Object2IntLinkedOpenHashMap<K> = OpenHashMap<K, i32, ObjectHash, true>;
pub type Int2LongOpenHashMap = OpenHashMap<i32, i64, IntHash>;
pub type Short2ObjectOpenHashMap<V> = OpenHashMap<i16, V, ShortHash>;
pub type Int2IntOpenHashMap = OpenHashMap<i32, i32, IntHash>;
pub type Long2FloatLinkedOpenHashMap = OpenHashMap<i64, f32, LongHash, true>;
pub type ReferenceLinkedOpenHashSet<K> = ReferenceOpenHashSet<K>;
pub type Short2BooleanOpenHashMap = OpenHashMap<i16, bool, ShortHash>;
pub type Long2LongLinkedOpenHashMap = OpenHashMap<i64, i64, LongHash, true>;
pub type Long2BooleanOpenHashMap = OpenHashMap<i64, bool, LongHash>;
pub type Reference2FloatOpenHashMap<K> = ReferenceHashMap<K, f32>;
pub type Int2IntLinkedOpenHashMap = OpenHashMap<i32, i32, IntHash, true>;
pub type Int2DoubleOpenHashMap = OpenHashMap<i32, f64, IntHash>;
pub type CharOpenHashSet = OpenHashSet<u16, CharHash>;
pub type ShortOpenHashSet = OpenHashSet<i16, ShortHash>;

pub type HashMap<K, V> = JavaHashMap<K, V>;
pub type HashSet<K> = JavaHashSet<K>;
pub type LinkedHashMap<K, V> = JavaHashMap<K, V, true>;
pub type LinkedHashSet<K> = JavaHashSet<K, true>;
pub type IdentityHashMap<K, V> = ReferenceHashMap<K, V>;
