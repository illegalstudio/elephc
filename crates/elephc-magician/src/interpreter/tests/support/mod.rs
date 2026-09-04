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

/// First PHP resource id the fake registry mints, matching the runtime's cursor.
///
/// `elephc::codegen_support::runtime::resource_ids` starts `_resource_id_next` at
/// 5 because PHP 8.5.6 reserves 1, 2 and 3 for the standard streams and 4 for the
/// CLI's own handle; the first `fopen()` in a program reports 5.
pub(super) const FAKE_FIRST_RESOURCE_ID: i64 = 5;

/// Highest payload the fake registry answers as `payload + 1` without minting.
///
/// Mirrors `STD_STREAM_MAX_PAYLOAD` in the runtime emitter: descriptors 0, 1 and 2
/// are STDIN/STDOUT/STDERR and PHP fixes their ids at 1, 2 and 3.
pub(super) const FAKE_STD_STREAM_MAX_PAYLOAD: i64 = 2;

/// Test runtime hooks that allocate stable fake handles and record echo output.
#[derive(Default)]
pub(super) struct FakeOps {
    pub(super) next_id: usize,
    pub(super) values: HashMap<usize, FakeValue>,
    /// Fake mirror of the runtime resource-id registry: native payload to PHP id.
    ///
    /// WHY THIS EXISTS. The fake used to answer `payload + 1` for every resource
    /// coercion, which was the pre-registry runtime invariant and stopped being
    /// true when `resource_ids.rs` landed. That made every test here structurally
    /// blind to resource numbering: eval hands out payloads from a counter of its
    /// own, so `payload + 1` produced a plausible small consecutive sequence no
    /// matter what the real runtime would have printed. Modelling the registry —
    /// bind-if-absent at creation, standard-stream shortcut, ids never reused —
    /// makes a fake coercion agree with what a compiled program actually prints.
    pub(super) resource_ids: HashMap<i64, i64>,
    /// Next never-used PHP resource id, lazily initialized to `FAKE_FIRST_RESOURCE_ID`.
    pub(super) resource_id_next: i64,
    /// Resource payloads that must NEVER receive a PHP resource id.
    ///
    /// Fake mirror of resource KIND 5, the eval-owned inert hash context. PHP 8's
    /// `hash_init()` returns a `HashContext` OBJECT, so it draws from the object-handle
    /// space and consumes nothing from the resource counter; the real runtime models
    /// this by having `__rt_mixed_from_value` skip `__rt_resource_id_of` for kind 5.
    /// Without this set the fake would bind an id for every hash context — `alloc`
    /// binds one for EVERY `FakeValue::Resource` — and every magician test would stay
    /// structurally unable to observe the very bug this models, because the fake's
    /// counter would keep advancing exactly as the buggy runtime's did.
    pub(super) inert_resources: std::collections::HashSet<i64>,
    /// Host resource payloads whose registry slot is no longer `Live`.
    ///
    /// Fake mirror of `__rt_resource_lookup_any` answering a slot whose status is not
    /// `RESOURCE_STATUS_LIVE`. Since the generation-safe registry migration an OPEN host
    /// payload is an opaque handle and `fclose` publishes the closed state on the slot
    /// instead of stamping a sentinel into the box, so nothing about the payload word
    /// itself separates an open handle from a closed one. A fake that answered "open" for
    /// every non-negative payload would be structurally unable to see that.
    pub(super) closed_resources: std::collections::HashSet<i64>,
    pub(super) object_classes: HashMap<usize, String>,
    pub(super) output: String,
    pub(super) releases: Vec<RuntimeCellHandle>,
    pub(super) warnings: Vec<String>,
    /// `@` suppression depth; while non-zero, `warning()` records nothing.
    pub(super) suppress_depth: u32,
    pub(super) fail_array_set_call: Option<usize>,
    pub(super) array_set_calls: usize,
    pub(super) ob_stack: Vec<FakeObLevel>,
    pub(super) ob_implicit_flush: bool,
}

/// One fake output-buffer level: captured text plus the ob_start metadata the
/// status/introspection builtins report.
#[derive(Default)]
pub(super) struct FakeObLevel {
    pub(super) buffer: String,
    pub(super) name: String,
    pub(super) chunk_size: i64,
    pub(super) flags: i64,
}

impl FakeOps {
    /// Allocates one fake runtime cell and returns its opaque handle.
    ///
    /// This is the fake's counterpart of `__rt_mixed_from_value`, so it is also
    /// where a resource payload acquires its PHP id: boxing is the one point every
    /// resource passes through in the real runtime, and binding here reproduces
    /// both halves of that contract — creation order decides the id, and re-boxing
    /// a payload that already has one (`$b = $a`) keeps it.
    pub(super) fn alloc(&mut self, value: FakeValue) -> RuntimeCellHandle {
        if let FakeValue::Resource(payload) = value {
            self.bind_resource_id(payload);
        }
        self.next_id += 1;
        let id = self.next_id;
        self.values.insert(id, value);
        RuntimeCellHandle::from_raw(id as *mut RuntimeCell)
    }

    /// Records a payload as inert, so no PHP resource id is ever bound to it.
    ///
    /// The fake counterpart of boxing with resource kind 5. Must be called BEFORE the
    /// `alloc` that creates the cell, because `alloc` is where binding happens.
    pub(super) fn mark_resource_inert(&mut self, payload: i64) {
        self.inert_resources.insert(payload);
    }

    /// Binds a fresh PHP resource id to a native payload that does not have one.
    ///
    /// Mirrors `__rt_resource_id_of`: the standard stream descriptors answer
    /// `payload + 1` without consuming the counter, every other payload takes the
    /// next never-used id, and ids are never reused. Inert payloads (kind 5, the eval
    /// hash context) are skipped entirely, which is what keeps `hash_init()` from
    /// shifting the ids of resources created after it.
    fn bind_resource_id(&mut self, payload: i64) {
        if self.inert_resources.contains(&payload) {
            return;
        }
        if payload <= FAKE_STD_STREAM_MAX_PAYLOAD || self.resource_ids.contains_key(&payload) {
            return;
        }
        if self.resource_id_next == 0 {
            self.resource_id_next = FAKE_FIRST_RESOURCE_ID;
        }
        self.resource_ids.insert(payload, self.resource_id_next);
        self.resource_id_next += 1;
    }

    /// Returns the PHP resource id bound to a native payload.
    ///
    /// Panics rather than falling back to arithmetic on the payload: an unbound
    /// payload means a resource cell reached a display path without going through
    /// `alloc`, and a silent fallback here is precisely the drift that made the
    /// old `payload + 1` model look correct.
    ///
    /// AN INERT PAYLOAD PANICS TOO, and deliberately with a different message. The
    /// real runtime does NOT panic there: `__rt_resource_id_of` still mints lazily for
    /// a kind-5 cell that reaches a display path, which is what guarantees no path can
    /// ever print a raw address. The fake cannot do that because this method — and all
    /// three of its callers in `super::conversions` — take `&self`. Widening them to
    /// `&mut self` is the honest fix if a test ever needs it; a silent fallback here
    /// would re-introduce exactly the blindness this fake was rewritten to remove.
    ///
    /// A NEGATIVE PAYLOAD IS A CLOSED HANDLE AND ANSWERS `-payload`, mirroring the
    /// `tbnz x0, #63` / `js` arm the real helper grew (`resource_ids.rs`) when `fclose`,
    /// `pclose` and `closedir` started stamping `-id` into the box. Without this arm the
    /// fake fell straight into the standard-stream shortcut below, because every negative
    /// value is `<= 2`: a `FakeValue::Resource(-5)` cell answered `-4`, so a unit test for
    /// the closed-resource display path would have asserted
    /// `resource(-4) of type (Unknown)` and passed.
    pub(super) fn fake_resource_id(&self, payload: i64) -> i64 {
        if self.inert_resources.contains(&payload) {
            panic!(
                "inert resource payload {payload} reached a display path; the runtime \
                 mints lazily here, so make `fake_resource_id` take `&mut self` and \
                 bind on demand rather than adding a fallback"
            );
        }
        if payload < 0 {
            return -payload;
        }
        if payload <= FAKE_STD_STREAM_MAX_PAYLOAD {
            return payload + 1;
        }
        *self
            .resource_ids
            .get(&payload)
            .expect("fake resource payload was never bound to an id")
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
