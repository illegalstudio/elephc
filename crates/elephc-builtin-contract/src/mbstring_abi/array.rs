//! Purpose:
//! Defines owned array graphs crossing the shared mbstring wire boundary.
//!
//! Called from:
//! - Native/eval array adapters and the dependency-free mbstring codec engine.
//!
//! Key details:
//! - Graph identities retain cycles and aliases without exporting host heap pointers.
//! - Keys retain insertion order and distinguish numeric strings from integer keys.
//! - Scalars preserve binary strings and raw float bits; unsupported values are explicit.

mod wire;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

/// A PHP array key after ordinary source key normalization has already occurred.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key { Int(i64), String(Vec<u8>) }

/// Scalar payloads and array identities, preserving float bits and binary string data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),
    String(Vec<u8>),
    Array(usize),
    Unsupported,
}

/// Insertion-ordered entries whose keys have PHP's exact integer or string identity.
pub type Array = Vec<(Key, Value)>;

/// A validated graph with one array root and no dangling array identities or duplicate keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayGraph { root: usize, arrays: Vec<Array> }

impl ArrayGraph {
    /// Validates every node, including unreachable nodes, before exposing a borrowed graph.
    pub fn new(root: usize, arrays: Vec<Array>) -> Option<Self> {
        if root >= arrays.len() { return None; }
        for entries in &arrays {
            let mut keys = HashSet::new();
            for (key, value) in entries {
                if !keys.insert(key) { return None; }
                if matches!(value, Value::Array(index) if *index >= arrays.len()) { return None; }
            }
        }
        Some(Self { root, arrays })
    }

    /// Returns the identity of the array passed to the PHP operation.
    pub fn root(&self) -> usize { self.root }

    /// Borrows all validated array nodes in identity order.
    pub fn arrays(&self) -> &[Array] { &self.arrays }

    /// Removes output nodes discarded by key collisions and renumbers retained identities.
    pub fn into_compact(self) -> Self {
        let mut remap = vec![None; self.arrays.len()];
        remap[self.root] = Some(0);
        let mut queue = vec![self.root];
        let mut arrays = Vec::new();
        let mut cursor = 0;
        while cursor < queue.len() {
            let mut entries = self.arrays[queue[cursor]].clone();
            for (_, value) in &mut entries {
                if let Value::Array(index) = value {
                    let next = *remap[*index].get_or_insert_with(|| {
                        let next = queue.len();
                        queue.push(*index);
                        next
                    });
                    *index = next;
                }
            }
            arrays.push(entries);
            cursor += 1;
        }
        Self { root: 0, arrays }
    }
}
