//! The read operations used by particle codecs, retaining JsonOps/NbtOps kinds.
use ferraloom_nbt::Tag;
use ferraloom_protocol::{
    component::Persistent,
    ops::{JsonOps, encode_nbt},
};
use serde_json::Value;
use std::fmt;

#[derive(Clone, Copy)]
pub enum Expected {
    Map,
    List,
    Number,
    String,
}
/// Format adapter for the single particle-options schema. Implemented for
/// serde_json::Value and NBT Tag; callers pass their existing parsed value.
pub trait Input: fmt::Display + Sized {
    fn is_map(&self) -> bool;
    fn get(&self, key: &str) -> Option<&Self>;
    fn list(&self) -> Option<Vec<Self>>;
    fn persistent<T: Persistent>(&self) -> ferraloom_protocol::Result<T>;
    fn to_json(&self) -> ferraloom_protocol::Result<Value>;
    fn type_error(&self, expected: Expected) -> String;
}
impl Input for Tag {
    fn is_map(&self) -> bool {
        self.as_compound().is_some()
    }
    fn get(&self, key: &str) -> Option<&Self> {
        self.as_compound()?.get(key)
    }
    fn list(&self) -> Option<Vec<Self>> {
        (0..self.collection_len()?)
            .map(|i| self.collection_get(i))
            .collect()
    }
    fn persistent<T: Persistent>(&self) -> ferraloom_protocol::Result<T> {
        T::from_nbt(self)
    }
    fn to_json(&self) -> ferraloom_protocol::Result<Value> {
        encode_nbt(self, &JsonOps)
    }
    fn type_error(&self, expected: Expected) -> String {
        match expected {
            Expected::Map => format!("Not a map: {self}"),
            Expected::List => format!("Not a list: {self}"),
            Expected::Number => "Not a number".into(),
            Expected::String => "Not a string".into(),
        }
    }
}
impl Input for Value {
    fn is_map(&self) -> bool {
        self.is_object()
    }
    fn get(&self, key: &str) -> Option<&Self> {
        // JsonOps MapLike#get treats JsonNull as a missing field.
        self.as_object()?.get(key).filter(|v| !v.is_null())
    }
    fn list(&self) -> Option<Vec<Self>> {
        self.as_array().cloned()
    }
    fn persistent<T: Persistent>(&self) -> ferraloom_protocol::Result<T> {
        T::from_json(self)
    }
    fn to_json(&self) -> ferraloom_protocol::Result<Value> {
        Ok(self.clone())
    }
    fn type_error(&self, expected: Expected) -> String {
        let kind = match expected {
            Expected::Map => "a JSON object",
            Expected::List => "a json array",
            Expected::Number => "a number",
            Expected::String => "a string",
        };
        format!("Not {kind}: {self}")
    }
}
