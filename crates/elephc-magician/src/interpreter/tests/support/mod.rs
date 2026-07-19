//! Purpose:
//! Shared fake runtime support for interpreter unit tests.
//! The fixtures allocate opaque runtime cells and implement `RuntimeValueOps`
//! without linking generated runtime hooks.
//!
//! Called from:
//! - `crate::interpreter::tests::*` focused test modules.
//!
//! Key details:
//! - Fake handles are stable integer-backed pointers used only inside tests.
//! - Output, warnings, and releases are recorded for assertions.

use std::collections::HashMap;
use std::ffi::c_void;

use crate::value::RuntimeCell;

use super::super::*;

mod array_ops;
mod cell_ops;
mod conversions;
mod lifecycle_ops;
mod numeric_ops;
mod object_ops;
mod runtime_ops;

/// Test-only array key representation for fake indexed and associative arrays.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum FakeKey {
    Int(i64),
    String(String),
}

/// Test-only runtime value representation used behind opaque cell handles.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum FakeValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<RuntimeCellHandle>),
    Assoc(Vec<(FakeKey, RuntimeCellHandle)>),
    Object(Vec<(String, RuntimeCellHandle)>),
    Iterator { len: i64, position: i64 },
    Resource(i64),
    InvokerRefCell(usize),
}

/// Test runtime hooks that allocate stable fake handles and record echo output.
#[derive(Default)]
pub(super) struct FakeOps {
    pub(super) next_id: usize,
    pub(super) values: HashMap<usize, FakeValue>,
    pub(super) object_classes: HashMap<usize, String>,
    pub(super) output: String,
    pub(super) releases: Vec<RuntimeCellHandle>,
    pub(super) warnings: Vec<String>,
    pub(super) fail_array_set_call: Option<usize>,
    pub(super) array_set_calls: usize,
    pub(super) ob_stack: Vec<String>,
    pub(super) ob_implicit_flush: bool,
}

impl FakeOps {
    /// Allocates one fake runtime cell and returns its opaque handle.
    pub(super) fn alloc(&mut self, value: FakeValue) -> RuntimeCellHandle {
        self.next_id += 1;
        let id = self.next_id;
        self.values.insert(id, value);
        RuntimeCellHandle::from_raw(id as *mut RuntimeCell)
    }

    /// Reads a fake runtime cell by opaque handle.
    pub(super) fn get(&self, handle: RuntimeCellHandle) -> FakeValue {
        let id = handle.as_ptr() as usize;
        self.values.get(&id).cloned().expect("fake cell missing")
    }

    /// Converts a fake runtime cell into a normalized fake PHP array key.
    pub(super) fn key(&self, handle: RuntimeCellHandle) -> Result<FakeKey, EvalStatus> {
        let value = self.get(handle);
        match value {
            FakeValue::Int(value) => Ok(FakeKey::Int(value)),
            FakeValue::String(value) => eval_numeric_string_array_key(value.as_bytes())
                .map(FakeKey::Int)
                .map_or_else(|| Ok(FakeKey::String(value)), Ok),
            FakeValue::Bytes(value) => eval_numeric_string_array_key(&value)
                .map(FakeKey::Int)
                .map_or_else(
                    || {
                        Ok(FakeKey::String(
                            String::from_utf8_lossy(&value).into_owned(),
                        ))
                    },
                    Ok,
                ),
            FakeValue::Null => Ok(FakeKey::String(String::new())),
            value => Ok(FakeKey::Int(self.fake_int(&value))),
        }
    }

    /// Allocates a fake runtime cell for an existing PHP array key.
    pub(super) fn alloc_key(&mut self, key: &FakeKey) -> Result<RuntimeCellHandle, EvalStatus> {
        match key {
            FakeKey::Int(value) => self.int(*value),
            FakeKey::String(value) => self.string(value),
        }
    }

    /// Finds a fake object property by insertion-order name.
    pub(super) fn object_property(
        properties: &[(String, RuntimeCellHandle)],
        name: &str,
    ) -> Option<RuntimeCellHandle> {
        properties
            .iter()
            .find_map(|(property, value)| (property == name).then_some(*value))
    }

    /// Configures one fake array-set call to fail for cleanup-path tests.
    pub(super) fn fail_array_set_call(&mut self, call_index: usize) {
        self.fail_array_set_call = Some(call_index);
        self.array_set_calls = 0;
    }
}

/// Test native invoker that returns the descriptor pointer as a runtime cell.
pub(super) unsafe extern "C" fn fake_native_return_descriptor(
    descriptor: *mut c_void,
    _args: *mut RuntimeCell,
) -> *mut RuntimeCell {
    descriptor.cast()
}
