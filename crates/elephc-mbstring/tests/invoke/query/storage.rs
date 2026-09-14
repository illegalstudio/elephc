//! Purpose:
//! Models live query output tables independently of the shared name parser and native heap.
//!
//! Called from:
//! - Protected query replay callbacks consuming normalized wire instructions.
//!
//! Key details:
//! - Root aliases share writes; existing nested arrays separate when traversed through a copy.
//! - Appends preserve signed PHP next-index history and never replace a maximum-index entry.

use super::*;
use elephc_builtin_contract::mbstring_abi::array::Key;

/// Observable query values retaining array identity independently of their snapshots.
#[derive(Clone)]
pub(super) enum Output { Null, String(Vec<u8>), Array(Rc<RefCell<Table>>) }

/// A PHP-like table with insertion ordering and a signed append counter.
#[derive(Clone)]
pub(super) struct Table { entries: Vec<(Key, Output)>, next: i64 }

impl Output {
    /// Creates the fresh empty construction array exposed by successful output initialization.
    pub(super) fn array() -> Self { Self::Array(Rc::new(RefCell::new(Table { entries: Vec::new(), next: i64::MIN }))) }

    /// Copies observable content without retaining additional live array references in the oracle log.
    pub(super) fn snapshot(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::String(bytes) => encoded(bytes),
            Self::Array(table) => json!({"array": table.borrow().entries.iter().map(|(key, value)| {
                let key = match key { Key::Int(index) => json!(index), Key::String(bytes) => encoded(bytes) };
                json!([key, value.snapshot()])
            }).collect::<Vec<_>>() }),
        }
    }

    /// Applies already-normalized wire steps while keeping host storage rules out of name parsing.
    pub(super) unsafe fn apply(&self, steps: &[MbQueryStepV1], value: &[u8]) -> bool {
        let Self::Array(root) = self else { return false; };
        let mut current = root.clone();
        for step in steps {
            let key = if step.append == 1 {
                let table = current.borrow();
                let key = Key::Int(if table.next == i64::MIN { 0 } else { table.next });
                if table.entries.iter().any(|(existing, _)| *existing == key) { return false; }
                key
            } else { unsafe { read_key(&step.key) } };
            if step.operation == QUERY_REMOVE_ROOT {
                root.borrow_mut().entries.retain(|(existing, _)| *existing != key);
                return true;
            }
            if step.operation == QUERY_STORE {
                current.borrow_mut().insert(key, Self::String(value.to_vec()));
                return false;
            }
            assert_eq!(step.operation, QUERY_ENTER);
            let child = {
                let mut table = current.borrow_mut();
                if !table.entries.iter().any(|(existing, child)| *existing == key && matches!(child, Self::Array(_))) {
                    table.insert(key.clone(), Self::array());
                }
                let (_, Self::Array(child)) = table.entries.iter_mut().find(|(existing, _)| *existing == key).unwrap() else { unreachable!() };
                if Rc::strong_count(child) > 1 {
                    let detached = child.borrow().clone();
                    *child = Rc::new(RefCell::new(detached));
                }
                child.clone()
            };
            current = child;
        }
        false
    }
}

impl Table {
    /// Updates a normalized key while preserving order and monotonic signed append history.
    fn insert(&mut self, key: Key, value: Output) {
        if let Key::Int(index) = &key { if *index >= self.next { self.next = index.saturating_add(1); } }
        if let Some((_, old)) = self.entries.iter_mut().find(|(existing, _)| *existing == key) { *old = value; }
        else { self.entries.push((key, value)); }
    }
}

/// Reads only the key representations promised by the shared query wire planner.
unsafe fn read_key(key: &MbHostValueV1) -> Key {
    match key.tag {
        HOST_INT => Key::Int(key.lo as i64),
        HOST_STRING => Key::String(unsafe { std::slice::from_raw_parts(key.lo as *const u8, key.hi as usize) }.to_vec()),
        tag => panic!("invalid query key {tag}"),
    }
}
