//! Purpose:
//! Defines protected host configuration, filtering, and live writes for mb_parse_str.
//!
//! Called from:
//! - The shared query invocation coordinator and native/eval output-reference adapters.
//!
//! Key details:
//! - V5 keeps the V4 prefix and its output initialization/owner lifecycle unchanged.
//! - Shared registration steps carry normalized keys; hosts own storage, COW, and append counters.
//! - Callback status and completed mutation metadata remain independent during pending exceptions.

use super::{c_void, MbHostStringV1, MbHostValueV1, MbInvokeHostV4};

mod storage;
pub use storage::*;

/// Initial configuration phase, returning owned separator bytes and the whole-query variable limit.
pub const QUERY_CONFIG_ENTRY: u32 = 0;
/// Per-field configuration phase, read after filtering for the current nesting limit.
pub const QUERY_CONFIG_FIELD: u32 = 1;
/// Diagnostic phase, read after root removal and its destructor-visible core-setting changes.
pub const QUERY_CONFIG_DIAGNOSTIC: u32 = 2;

/// Core parser settings; any separator owner must be published before returning callback status.
/// Entry reads separator/max_variables; field reads max_nesting; diagnostic reads display_errors.
/// Separator bytes are borrowed until their owner is released, or until the next host callback
/// when owner is null. display_errors is exactly zero or one. Limits are signed PHP integers.
#[repr(C)]
pub struct MbQueryConfigV1 {
    pub separators: MbHostStringV1,
    pub max_variables: i64,
    pub max_nesting: i64,
    pub display_errors: u64,
}

/// Reads core configuration without executing PHP or changing mbstring request state.
pub type MbQueryConfigurationV1 = unsafe extern "C" fn(*mut c_void, u32, *mut MbQueryConfigV1) -> i32;

/// A filtered binary value and its independent acceptance flag, always initialized before status.
/// Accepted is zero or one. An owner is released even for rejected values or pending exceptions.
#[repr(C)]
pub struct MbQueryFilteredV1 { pub accepted: u64, pub value: MbHostStringV1 }

/// Applies the SAPI PARSE_STRING input filter to borrowed converted name/value bytes.
/// A successful callback returns the accepted replacement value or rejects this field.
/// Native hosts without an installed filter can supply null, selecting the identity filter.
pub type MbQueryFilterV1 = unsafe extern "C" fn(
    *mut c_void, *const u8, u64, *const u8, u64, *mut MbQueryFilteredV1,
) -> i32;

/// Native query invocation state for a resolved persistent output reference and live host policy.
/// The capture prefix owns the same initialization mode and displaced-value handoff as V4.
/// Configuration is required; a missing callback disables query output before argument cloning.
/// A null filter selects identity filtering. Policy callbacks contain PHP exceptions and use
/// `context`, independently of the invocation's eval context. Except for capture.discarded,
/// the caller keeps this record immutable and its callback/context owners alive until return.
#[repr(C)]
pub struct MbNativeQueryV1 {
    pub capture: super::MbNativeCaptureV1,
    pub context: *mut c_void,
    pub configuration: Option<MbQueryConfigurationV1>,
    pub filter: Option<MbQueryFilterV1>,
}

/// Enters a named or appended array child of the current table.
pub const QUERY_ENTER: u64 = 1;
/// Stores the filtered value in the current table, with None represented by the append flag.
pub const QUERY_STORE: u64 = 2;
/// Removes a root key after an input nesting overflow.
pub const QUERY_REMOVE_ROOT: u64 = 3;

/// One borrowed shared name-planner instruction; append is zero or one.
/// Named keys are HOST_INT/HOST_STRING descriptors. Appends ignore the zeroed key descriptor.
/// All descriptors and pointed-to bytes remain borrowed only through the register callback.
#[repr(C)]
pub struct MbQueryStepV1 { pub operation: u64, pub append: u64, pub key: MbHostValueV1 }

/// Reports whether execution actually reached the RemoveRoot instruction, independently of status.
/// The host sets nesting_exceeded to zero before execution and one only after that mutation.
#[derive(Default)]
#[repr(C)]
pub struct MbQueryRegisteredV1 { pub nesting_exceeded: u64 }

/// Applies ordered shared registration steps to the writer's live PHP output array.
/// Each invocation starts at the current root. A non-array root ignores the field. Enter
/// preserves existing arrays with PHP's nested COW behavior or replaces a scalar with an
/// empty array. Failed appends stop this field before any later step. Root writes retain
/// exposed construction-array aliases; do not replace a whole output snapshot or separate
/// the root merely because a callback retained it. Continue completed PHP mutations after
/// status two, containing exceptions and preserving their runtime chain. Fatal host errors
/// stop further work. Every internal cursor/temporary owner is retired before returning.
pub type MbQueryRegisterV1 = unsafe extern "C" fn(
    *mut c_void, *mut c_void, *const MbQueryStepV1, u64, *const u8, u64, *mut MbQueryRegisteredV1,
) -> i32;

/// Adds query configuration and live registration to V4; filter is an optional identity override.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeHostV5 {
    pub base: MbInvokeHostV4,
    pub query_configuration: Option<MbQueryConfigurationV1>,
    pub query_filter: Option<MbQueryFilterV1>,
    pub query_register: Option<MbQueryRegisterV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the shared 64-bit layout used by every supported target and the complete V4 prefix.
    #[test]
    fn mbstring_query_host_layout() {
        assert_eq!(std::mem::size_of::<MbNativeQueryV1>(), 40);
        assert_eq!(std::mem::offset_of!(MbNativeQueryV1, capture), 0);
        assert_eq!(std::mem::offset_of!(MbNativeQueryV1, context), 16);
        assert_eq!(std::mem::offset_of!(MbNativeQueryV1, configuration), 24);
        assert_eq!(std::mem::offset_of!(MbNativeQueryV1, filter), 32);
        assert_eq!(std::mem::size_of::<MbInvokeHostV5>(), 144);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV5, base), 0);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV5, query_configuration), 120);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV5, query_filter), 128);
        assert_eq!(std::mem::offset_of!(MbInvokeHostV5, query_register), 136);
        assert_eq!(std::mem::size_of::<MbQueryConfigV1>(), 48);
        assert_eq!(std::mem::size_of::<MbQueryFilteredV1>(), 32);
        assert_eq!(std::mem::size_of::<MbQueryStepV1>(), 40);
        assert_eq!(std::mem::size_of::<MbQueryRegisteredV1>(), 8);
    }
}
