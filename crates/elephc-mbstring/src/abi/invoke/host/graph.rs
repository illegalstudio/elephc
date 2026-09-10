//! Purpose:
//! Snapshots exact-key array graphs through protected owned host reads.
//!
//! Called from:
//! - Shared invocation after outer coercions and source-list callbacks complete.
//!
//! Key details:
//! - Traversal is depth first and each native array identity is visited once.
//! - Session ownership survives callbacks, malformed metadata, and Rust panics.
//! - No PHP callback runs with request state borrowed; old hosts retain their raw reader path.

use std::collections::{HashMap, HashSet};
use elephc_builtin_contract::mbstring_abi::array::{Array, ArrayGraph, Key, Value};
use crate::coercion::Input;
use super::*;

/// Retains one graph node's owned source and guards against nonadvancing host cursors.
struct Node {
    source: MbArraySourceV2,
    cursor: u64,
    seen: HashSet<u64>,
    entries: Array,
}

impl Node {
    /// Starts one native array at its first insertion-ordered entry.
    fn new(source: MbArraySourceV2) -> Self {
        Self { source, cursor: 0, seen: HashSet::from([0]), entries: Vec::new() }
    }
}

impl Session {
    /// Copies an argument graph while resolving current references through protected V3 callbacks.
    pub(in crate::abi::invoke) unsafe fn snapshot_argument(
        &mut self, index: usize, root: MbHostValueV1,
    ) -> Result<ArrayGraph, Status> {
        let Some(callback) = self.graph_value else {
            return unsafe { super::super::super::snapshot::snapshot(root, self.reader(), self.context()) }
                .ok_or(Status::Fatal);
        };
        if !matches!(root.tag, HOST_INDEXED_ARRAY | HOST_ASSOC_ARRAY) { return Err(Status::Fatal); }
        let mut identities = HashMap::from([((root.tag, root.lo), 0)]);
        let mut nodes = vec![Node::new(MbArraySourceV2 {
            retained: self.arguments[index], original: self.originals[index],
        })];
        let mut stack = vec![0];
        while let Some(&node) = stack.last() {
            self.graph_entries.push(MbArrayEntryV3::default());
            let slot = self.graph_entries.len() - 1;
            let source = nodes[node].source;
            let status = unsafe { callback(self.context(), &source, &mut nodes[node].cursor, &mut self.graph_entries[slot]) };
            Status::decode(status)?;
            let entry = &self.graph_entries[slot];
            if entry.kind == ITER_END && entry.owner.is_null() && entry.original.is_null() {
                stack.pop();
                continue;
            }
            let cursor = nodes[node].cursor;
            if entry.kind != ITER_ENTRY || entry.owner.is_null() || !nodes[node].seen.insert(cursor) {
                return Err(Status::Fatal);
            }
            let key = unsafe { copy_key(entry.key) }.ok_or(Status::Fatal)?;
            let source = MbArraySourceV2 { retained: entry.owner, original: entry.original };
            let input = unsafe { self.describe_owner(source.retained)? };
            let decoded = unsafe { super::super::super::coercion::decode_input(&input) }.ok_or(Status::Fatal)?;
            let mut child = None;
            let value = match decoded {
                Input::Null => Value::Null,
                Input::Bool(value) => Value::Bool(value),
                Input::Int(value) => Value::Int(value),
                Input::Float(bits) => Value::Float(bits),
                Input::String(bytes) => Value::String(bytes.to_vec()),
                Input::Object { .. } | Input::Resource { .. } => Value::Unsupported,
                Input::Array => {
                    let identity = *identities.entry((input.kind, input.value)).or_insert_with(|| {
                        let identity = nodes.len();
                        nodes.push(Node::new(source));
                        child = Some(identity);
                        identity
                    });
                    Value::Array(identity)
                },
            };
            unsafe { self.release_temporary()?; }
            nodes[node].entries.push((key, value));
            if let Some(child) = child { stack.push(child); }
        }
        ArrayGraph::new(0, nodes.into_iter().map(|node| node.entries).collect()).ok_or(Status::Fatal)
    }
}

/// Copies a key before another callback can invalidate its borrowed native byte range.
unsafe fn copy_key(key: MbHostValueV1) -> Option<Key> {
    match key.tag {
        HOST_INT => Some(Key::Int(key.lo as i64)),
        HOST_STRING if key.hi <= isize::MAX as u64 && (key.hi == 0 || key.lo != 0) => {
            let bytes = if key.hi == 0 { Vec::new() }
                else { unsafe { std::slice::from_raw_parts(key.lo as *const u8, key.hi as usize).to_vec() } };
            Some(Key::String(bytes))
        },
        _ => None,
    }
}
