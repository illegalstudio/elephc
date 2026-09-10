//! Purpose:
//! Defines protected initialization and filling of caller-owned mbregex capture outputs.
//!
//! Called from:
//! - The shared capture invocation coordinator and its native/eval host adapters.
//!
//! Key details:
//! - Output references retain identity without making a copy of their previous PHP value.
//! - Successful initialization and pending destructor exceptions are independent states.
//! - Writers preserve the exposed construction array and its aliases across capture insertion.

use super::{c_void, MbHostValueV1, MbInvokeHostV3};

/// Initialization publishes a writer owner before reporting either success or failure.
/// `ready` is zero or one. One means the body may continue even with a pending throwable;
/// zero means initialization failed, for example because a typed property rejects an array.
/// Every nonnull writer is consumed once through capture_release, regardless of status.
#[derive(Default)]
#[repr(C)]
pub struct MbCaptureOutputV1 {
    pub ready: u64,
    pub writer: *mut c_void,
}

/// Untyped reference initialization exposes null during destruction, then publishes the fresh array.
pub const CAPTURE_REFERENCE_UNTYPED: u64 = 0;

/// Typed reference initialization publishes the fresh array before destruction; the host checks types first.
pub const CAPTURE_REFERENCE_TYPED: u64 = 1;

/// Internal native-reference initialization result, before the host adopts deferred request ownership.
/// `output.writer` owns one persistent reference, without copying its former value.
/// `discarded` owns a value installed by a destructor and subsequently overwritten by untyped
/// initialization. The host must preserve its PHP request lifetime separately from writer cleanup.
/// This larger result is not the V4 callback's sixteen-byte output and must never alias that buffer.
#[derive(Default)]
#[repr(C)]
pub struct MbCaptureReferenceInitV1 {
    pub output: MbCaptureOutputV1,
    pub discarded: *mut c_void,
}

/// Native capture invocation state for an already resolved persistent output reference.
/// The caller supplies a reviewed initialization mode and fresh zeroed ownership storage.
/// Initialization transfers any overwritten reentrant value into `discarded`, including
/// when it leaves a pending throwable. The generated native invocation adopts that owner
/// into request storage and clears this field before returning. Custom hosts must arrange
/// equivalent request-lifetime adoption. This state supplies no PHP type validation.
#[derive(Default)]
#[repr(C)]
pub struct MbNativeCaptureV1 {
    pub mode: u64,
    pub discarded: *mut c_void,
}

/// Initializes the caller's exact output reference after string parameter coercions.
/// The argument is pinned through final cleanup and must not be copied by value.
/// Typed constraints are checked before replacing the old value. Untyped initialization
/// exposes null to the old value's destructor, then publishes the fresh array. An assignable
/// typed reference publishes the fresh array before releasing the old value.
/// Status 2 may accompany ready=1 when that destructor leaves a pending throwable.
pub type MbCaptureInitializeV1 = unsafe extern "C" fn(
    *mut c_void, *const c_void, *mut MbCaptureOutputV1,
) -> i32;

/// Inserts an ordered ArrayGraph of numeric/named string-or-false captures into the writer.
/// The graph is borrowed only for this callback. Preserve existing unrelated entries and
/// aliases of the active construction array, including copies made during initialization.
/// The writer retains the PHP reference identity and resolves its current array for every
/// entry, including after a destructor replaces that reference's value. Pinning a writer
/// must not add a value owner that changes PHP copy-on-write behavior during initialization.
/// Do not replace the output wholesale or perform ordinary COW separation. Insert all
/// entries in order even when destruction of overwritten values leaves a throwable pending.
/// All PHP callbacks are contained; statuses retain the shared success/fatal/pending meaning.
pub type MbCaptureFillV1 = unsafe extern "C" fn(*mut c_void, *mut c_void, *const u8, u64) -> i32;

/// Stores one borrowed capture key/value pair in the writer's current PHP array.
/// Keys are integer or binary string descriptors; values are binary strings or false.
/// The callback completes this write before returning even if old-value destruction throws.
/// It contains PHP exceptions, returning zero for success, two for a pending throwable,
/// and one for an unrecoverable host failure. Later entries still run after status two.
/// Descriptor storage and string bytes remain borrowed only until this callback returns.
pub type MbCaptureStoreV1 = unsafe extern "C" fn(
    *mut c_void, *mut c_void, *const MbHostValueV1, *const MbHostValueV1,
) -> i32;

/// Consumes an opaque construction-writer owner behind the host's exception boundary.
/// Its lifetime ends before argument pins are released, including after failure or Rust panic.
pub type MbCaptureReleaseV1 = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;

/// Extends V3 with output-reference construction; the base header has version 4 and full size.
/// All three callbacks are required. Existing V1/V2/V3 callers retain their original contract.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeHostV4 {
    pub base: MbInvokeHostV3,
    pub capture_initialize: Option<MbCaptureInitializeV1>,
    pub capture_fill: Option<MbCaptureFillV1>,
    pub capture_release: Option<MbCaptureReleaseV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the V3 prefix and capture callback/output offsets shared by every supported target.
    #[test]
    fn mbstring_capture_host_layout() {
        assert_eq!(std::mem::size_of::<MbInvokeHostV4>(), 120);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV4, base), 0);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV4, capture_initialize), 96);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV4, capture_fill), 104);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV4, capture_release), 112);
        assert_eq!(std::mem::size_of::<MbCaptureOutputV1>(), 16);
        assert_eq!(std::mem::offset_of!(MbCaptureOutputV1, ready), 0);
        assert_eq!(std::mem::offset_of!(MbCaptureOutputV1, writer), 8);
        assert_eq!(std::mem::size_of::<MbCaptureReferenceInitV1>(), 24);
        assert_eq!(std::mem::offset_of!(MbCaptureReferenceInitV1, output), 0);
        assert_eq!(std::mem::offset_of!(MbCaptureReferenceInitV1, discarded), 16);
        assert_eq!(std::mem::size_of::<MbNativeCaptureV1>(), 16);
        assert_eq!(std::mem::offset_of!(MbNativeCaptureV1, mode), 0);
        assert_eq!(std::mem::offset_of!(MbNativeCaptureV1, discarded), 8);
        assert_eq!(CAPTURE_REFERENCE_UNTYPED, 0);
        assert_eq!(CAPTURE_REFERENCE_TYPED, 1);
    }
}
