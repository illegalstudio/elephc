//! Purpose:
//! Defines protected live-variable traversal callbacks for mb_convert_variables.
//!
//! Called from:
//! - The shared mbstring conversion coordinator and native/eval runtime adapters.
//!
//! Key details:
//! - Handles borrow caller storage and retain its lvalue identity without adding a COW owner.
//! - Children are inspected after their parent array has been separated for writes.
//! - Callbacks contain PHP exceptions and use the common success/fatal/pending statuses.

use std::ffi::c_void;

use super::invoke::MbInvokeHostV5;

/// Opaque host storage for one writable root or nested array/object value slot.
/// Roots retain caller storage and are dereferenced by the host. Nested handles
/// retain their original slot identity so replacing a string can detach a nested reference.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct MbVariableHandleV1 { pub words: [u64; 4] }

impl MbVariableHandleV1 {
    /// Selects one caller-owned root storage address for a synchronous V6 invocation.
    /// Hosts interpret word zero as the original argument address and word one as the root marker.
    pub fn root(storage: *const c_void) -> Option<Self> {
        (!storage.is_null()).then_some(Self { words: [storage as usize as u64, 1, 0, 0] })
    }
}

/// A scalar, string, array, or object observed after following reference cells.
/// String bytes remain borrowed and readable until the next host callback.
/// Container identity is stable through one traversal phase and differs for COW-separated arrays.
#[derive(Default)]
#[repr(C)]
pub struct MbVariableViewV1 {
    pub kind: u64,
    pub identity: u64,
    pub bytes: *const u8,
    pub len: u64,
}

/// A value with no recursively visited children.
pub const VARIABLE_OTHER: u64 = 0;
/// A binary PHP string in bytes/len.
pub const VARIABLE_STRING: u64 = 1;
/// A PHP array whose identity is its current storage allocation.
pub const VARIABLE_ARRAY: u64 = 2;
/// A PHP object whose identity is its current object allocation.
pub const VARIABLE_OBJECT: u64 = 3;

/// A completed child result, or the end of an ordered container scan.
#[derive(Default)]
#[repr(C)]
pub struct MbVariableChildV1 { pub kind: u64, pub handle: MbVariableHandleV1 }

/// End of the container's values; keys are never converted.
pub const VARIABLE_CHILD_END: u64 = 0;
/// One array value or object property slot, including indirect private/typed properties.
pub const VARIABLE_CHILD_ENTRY: u64 = 1;

/// Reads a borrowed slot and follows its references without executing user PHP code.
pub type MbVariableInspectV1 = unsafe extern "C" fn(
    *mut c_void, *const MbVariableHandleV1, *mut MbVariableViewV1,
) -> i32;

/// Advances a cursor through array values or all object properties in storage order.
/// A successful entry advances the cursor; its handle remains valid while the parent
/// container is active. This callback must not copy a value or change array sharing.
pub type MbVariableChildNextV1 = unsafe extern "C" fn(
    *mut c_void, u64, u64, *mut u64, *mut MbVariableChildV1,
) -> i32;

/// Performs PHP array COW separation in the live slot and returns its new identity.
/// Objects retain their identity. The host preserves both original and current
/// containers until the traversal unprotects its active-recursion bookkeeping.
pub type MbVariablePrepareWriteV1 = unsafe extern "C" fn(
    *mut c_void, *const MbVariableHandleV1, u64, u64, *mut u64,
) -> i32;

/// Replaces one string in the exact writable slot using borrowed converted bytes.
/// Root arguments write through their references; nested slots replace their
/// reference wrapper, including indirect typed object properties where applicable.
pub type MbVariableWriteStringV1 = unsafe extern "C" fn(
    *mut c_void, *const MbVariableHandleV1, *const u8, u64,
) -> i32;

/// Extends the protected V5 invocation host with live PHP variable actions.
/// All callbacks are required for mb_convert_variables and unused by older operations.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeHostV6 {
    pub base: MbInvokeHostV5,
    pub variable_inspect: Option<MbVariableInspectV1>,
    pub variable_child_next: Option<MbVariableChildNextV1>,
    pub variable_prepare_write: Option<MbVariablePrepareWriteV1>,
    pub variable_write_string: Option<MbVariableWriteStringV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the 64-bit ABI offsets shared by the supported targets.
    #[test]
    fn variable_host_layout() {
        assert_eq!(std::mem::size_of::<MbVariableHandleV1>(), 32);
        assert_eq!(std::mem::size_of::<MbVariableViewV1>(), 32);
        assert_eq!(std::mem::size_of::<MbVariableChildV1>(), 40);
        assert_eq!(std::mem::size_of::<MbInvokeHostV6>(), 176);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV6, variable_inspect), 144);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV6, variable_child_next), 152);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV6, variable_prepare_write), 160);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV6, variable_write_string), 168);
    }
}
