//! Purpose:
//! Defines protected host callbacks for shared mbstring call preparation and execution.
//!
//! Called from:
//! - Native/eval adapters supplying MbInvokeHostV1 to elephc_mbstring_invoke_v1.
//!
//! Key details:
//! - Callbacks return raw RuntimeBuiltinStatus values and must never unwind through Rust.
//! - Value copies detach scalar/reference cells while retaining PHP array COW and object identity.
//! - Every published owner is consumed exactly once, including on callback failure.

use std::ffi::c_void;
use super::{coercion::{MbCoercionInputV1, MbHostStringV1}, host::{MbArrayNextV1, MbHostValueV1}};

mod capture;
pub use capture::*;
mod query;
pub use query::*;

/// Copies one borrowed PHP argument by value before any coercion callback executes.
/// Null input represents PHP null; success must publish a nonnull owned boxed value.
/// Internal reference markers are dereferenced here, never exposed to the planner.
pub type MbCloneValueV1 = unsafe extern "C" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32;

/// Describes an owned argument without executing PHP or traversing arrays.
/// Optional metadata ownership keeps the returned class-name bytes alive until release.
pub type MbDescribeValueV1 = unsafe extern "C" fn(
    *mut c_void, *const c_void, *mut MbCoercionInputV1, *mut *mut c_void,
) -> i32;

/// Converts an object in string context behind the host's exception boundary.
/// V2 hosts also accept array and resource casts for encoding-list entries. Scalar entries
/// are converted by the shared coordinator regardless of the outer caller strictness.
pub type MbStringableV1 = unsafe extern "C" fn(*mut c_void, *const c_void, *mut MbHostStringV1) -> i32;

/// Formats IEEE-754 bits using current PHP precision without emitting additional NaN diagnostics.
pub type MbFormatFloatV1 = unsafe extern "C" fn(*mut c_void, u64, *mut MbHostStringV1) -> i32;

/// Delivers one binary diagnostic before the next coercion or deferred action.
/// A throwing error handler returns PendingThrowable and terminates call preparation.
pub type MbDiagnosticV1 = unsafe extern "C" fn(*mut c_void, u32, *const u8, u64) -> i32;

/// Consumes one native owner, even on failure, behind a PHP exception boundary.
/// Destructors may reenter mbstring; pending exceptions must survive later cleanup callbacks.
pub type MbReleaseOwnerV1 = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;

/// Complete immutable callback table, alive with its context until invocation returns.
/// Every callback is required. Statuses 0/1/2 mean success/fatal/pending throwable;
/// other values fail closed. Callbacks may publish owners on failure for cleanup.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeHostV1 {
    pub version: u32,
    pub size: u32,
    pub context: *mut c_void,
    pub clone_value: Option<MbCloneValueV1>,
    pub describe_value: Option<MbDescribeValueV1>,
    pub stringable: Option<MbStringableV1>,
    pub format_float: Option<MbFormatFloatV1>,
    pub diagnostic: Option<MbDiagnosticV1>,
    pub release_owner: Option<MbReleaseOwnerV1>,
    pub array_next: Option<MbArrayNextV1>,
}

/// One array-iteration result whose published value is independently owned by the caller.
/// Kinds 0/1/2 mean end/entry/invalid metadata. An entry must advance its cursor.
/// Any published owner must be released even when the callback returns failure.
#[derive(Default)]
#[repr(C)]
pub struct MbArrayEntryV2 { pub kind: u64, pub owner: *mut c_void }

/// Pairs the retained array value with the caller's original boxed-handle identity.
/// `original` is an opaque lookup key only; a host must not dereference it after callbacks.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MbArraySourceV2 { pub retained: *const c_void, pub original: *const c_void }

/// Copies one ordered array element by value without performing a string cast.
/// The retained root remains borrowed; the original identity permits host-side reference lookup.
/// The copied entry can safely survive later Stringable calls and caller-side assignments.
pub type MbArrayValueV2 = unsafe extern "C" fn(
    *mut c_void, *const MbArraySourceV2, *mut u64, *mut MbArrayEntryV2,
) -> i32;

/// Supplies non-unwinding native ownership actions to eval reference resolution.
/// `publish_throw` consumes a boxed Throwable into native pending-exception ownership.
/// Once installed, a pending exception takes precedence over any later fatal cleanup status.
#[repr(C)]
pub struct MbArrayReferenceHooksV2 {
    pub clone_value: MbCloneValueV1,
    pub release_owner: MbReleaseOwnerV1,
    pub publish_throw: MbReleaseOwnerV1,
}

/// Extends the original callback table with owned array-element iteration for encoding lists.
/// The base version is 2 and its size covers this complete structure. Version-one callers
/// remain valid for operations that do not require element coercion.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeHostV2 { pub base: MbInvokeHostV1, pub array_value: Option<MbArrayValueV2> }

/// One protected graph entry with an owned value copy and an optional owned identity lease.
/// The key bytes borrow the retained parent until the next graph callback; callers copy them first.
/// `original` keeps the exact source box alive for nested reference metadata lookups, without
/// permitting callers to dereference it. Both owner fields must be consumed even on failure.
#[repr(C)]
pub struct MbArrayEntryV3 {
    pub kind: u64,
    pub owner: *mut c_void,
    pub key: MbHostValueV1,
    pub original: *mut c_void,
}

impl Default for MbArrayEntryV3 {
    /// Initializes every ownership slot before a protected host callback can publish a result.
    fn default() -> Self {
        Self { kind: 0, owner: std::ptr::null_mut(), key: MbHostValueV1::null(), original: std::ptr::null_mut() }
    }
}

/// Reads one exact-key entry and retains its nested reference identity without casting values.
pub type MbArrayValueV3 = unsafe extern "C" fn(
    *mut c_void, *const MbArraySourceV2, *mut u64, *mut MbArrayEntryV3,
) -> i32;

/// Pins a borrowed boxed identity before any user callbacks run, without copying its value.
/// Pinning must not execute PHP or mutate the borrowed value.
/// Stack-backed or null identities may publish no owner if their lifetime already covers the call.
/// An acquired owner is consumed through the host's protected release callback.
pub type MbPinValueV3 = MbCloneValueV1;

/// Extends V2 with recursive owned graph reads and stable original argument identities.
/// The base version is 3 and size covers the complete structure; V1/V2 remain supported.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeHostV3 {
    pub base: MbInvokeHostV2,
    pub graph_value: Option<MbArrayValueV3>,
    pub pin_value: Option<MbPinValueV3>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins every callback offset consumed by the two native emitters and independent C callers.
    #[test]
    fn mbstring_invoke_host_layout() {
        assert_eq!(std::mem::size_of::<MbInvokeHostV1>(), 72);
        assert_eq!(std::mem::size_of::<MbInvokeHostV2>(), 80);
        assert_eq!(std::mem::size_of::<MbInvokeHostV3>(), 96);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV3, graph_value), 80);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV3, pin_value), 88);
        assert_eq!(std::mem::size_of::<MbArrayEntryV3>(), 48);
        assert_eq!(std::mem::offset_of!(MbArrayEntryV3, kind), 0);
        assert_eq!(std::mem::offset_of!(MbArrayEntryV3, owner), 8);
        assert_eq!(std::mem::offset_of!(MbArrayEntryV3, key), 16);
        assert_eq!(std::mem::offset_of!(MbArrayEntryV3, original), 40);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV2, base), 0);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV2, array_value), 72);
        assert_eq!(std::mem::size_of::<MbArrayEntryV2>(), 16);
        assert_eq!(std::mem::size_of::<MbArraySourceV2>(), 16);
        assert_eq!(std::mem::offset_of!(MbArraySourceV2, retained), 0);
        assert_eq!(std::mem::offset_of!(MbArraySourceV2, original), 8);
        assert_eq!(std::mem::size_of::<MbArrayReferenceHooksV2>(), 24);
        assert_eq!(std::mem::offset_of!(MbArrayReferenceHooksV2, clone_value), 0);
        assert_eq!(std::mem::offset_of!(MbArrayReferenceHooksV2, release_owner), 8);
        assert_eq!(std::mem::offset_of!(MbArrayReferenceHooksV2, publish_throw), 16);
        assert_eq!(std::mem::offset_of!(MbArrayEntryV2, kind), 0);
        assert_eq!(std::mem::offset_of!(MbArrayEntryV2, owner), 8);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, version), 0);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, size), 4);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, context), 8);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, clone_value), 16);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, describe_value), 24);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, stringable), 32);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, format_float), 40);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, diagnostic), 48);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, release_owner), 56);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV1, array_next), 64);
    }
}
