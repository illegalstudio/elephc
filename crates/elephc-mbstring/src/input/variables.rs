//! Purpose:
//! Applies shared query registration plans to owned PHP array graphs.
//!
//! Called from:
//! - Shared HTTP parsing after each name/value conversion and host input filter.
//!
//! Key details:
//! - Name planning owns normalization; this host owns live append counters and table identity.
//! - A nesting overflow removes the root variable; the host decides whether to emit a warning.
//! - Graph nodes own bytes and remain independent of native or eval heap representations.

use crate::arrays::{Array, ArrayGraph, Key, Value};
use super::{registration, RegistrationStep};

/// One output table with a persistent signed next-index counter, including negative keys.
struct Table { entries: Array, next: i64 }

impl Table {
    /// Creates PHP's initial next-index sentinel before any numeric key has been inserted.
    fn new() -> Self { Self { entries: Vec::new(), next: i64::MIN } }

    /// Inserts or replaces one normalized key while preserving order and next-index history.
    fn insert(&mut self, key: Key, value: Value) {
        if let Key::Int(index) = &key {
            if *index >= self.next { self.next = index.saturating_add(1); }
        }
        if let Some((_, previous)) = self.entries.iter_mut().find(|(existing, _)| *existing == key) { *previous = value; }
        else { self.entries.push((key, value)); }
    }

    /// Chooses an append key without overwriting an occupied maximum integer entry.
    fn append_key(&self) -> Option<Key> {
        let key = Key::Int(if self.next == i64::MIN { 0 } else { self.next });
        (!self.entries.iter().any(|(existing, _)| *existing == key)).then_some(key)
    }
}

/// Incrementally registered query variables; snapshots can expose partial output to host callbacks.
pub struct Variables { tables: Vec<Table> }

impl Default for Variables {
    /// Starts a new empty result without sharing storage with any previous caller value.
    fn default() -> Self { Self { tables: vec![Table::new()] } }
}

impl Variables {
    /// Registers one converted pair; returns false only when the nesting limit removed its root.
    /// Invalid names, forbidden mangled prefixes, and exhausted append indices are silently ignored.
    pub fn register(&mut self, name: &[u8], value: Vec<u8>, max_nesting: i64) -> bool {
        let mut table = 0;
        for step in registration(name, max_nesting) {
            match step {
                RegistrationStep::Enter(key) => {
                    let Some(child) = self.child(table, key) else { return true; };
                    table = child;
                },
                RegistrationStep::Store(key) => {
                    if let Some(key) = key.or_else(|| self.tables[table].append_key()) {
                        self.tables[table].insert(key, Value::String(value));
                    }
                    return true;
                },
                RegistrationStep::RemoveRoot(root) => {
                    self.tables[0].entries.retain(|(key, _)| *key != root);
                    return false;
                },
            }
        }
        true
    }

    /// Copies the currently visible output graph while discarding nodes overwritten by later keys.
    pub fn snapshot(&self) -> ArrayGraph {
        ArrayGraph::new(0, self.tables.iter().map(|table| table.entries.clone()).collect()).expect("registered query graph").into_compact()
    }

    /// Moves the final output graph without retaining discarded arrays or previous scalar values.
    pub fn into_graph(self) -> ArrayGraph {
        ArrayGraph::new(0, self.tables.into_iter().map(|table| table.entries).collect()).expect("registered query graph").into_compact()
    }

    /// Creates an array child when a missing or scalar parent is traversed by bracket syntax.
    fn child(&mut self, table: usize, key: Option<Key>) -> Option<usize> {
        let key = key.or_else(|| self.tables[table].append_key())?;
        if let Some((_, Value::Array(index))) = self.tables[table].entries.iter().find(|(existing, _)| *existing == key) { return Some(*index); }
        let index = self.tables.len();
        self.tables.push(Table::new());
        self.tables[table].insert(key, Value::Array(index));
        Some(index)
    }

}
