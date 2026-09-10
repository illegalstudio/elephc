//! Purpose:
//! Defines the storage callbacks consumed by shared native query-step execution.
//!
//! Called from:
//! - The mbstring query executor and target-aware native callback table emitter.
//!
//! Key details:
//! - Successful root/enter callbacks publish one lifetime pin, independently of pending status.
//! - Pins preserve cursors without adding ordinary PHP copy-on-write owners.
//! - Every callback contains PHP exceptions and returns zero, fatal one, or pending two.

use super::{c_void, MbHostValueV1};

/// Resolves the writer's current root and publishes one lifetime pin, or null for a non-array.
/// The output starts null; a published pin must be released even after a failed callback.
pub type MbQueryRootV1 = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32;

/// An append decision independent of callback status; available must be zero or one.
#[derive(Default)]
#[repr(C)]
pub struct MbQueryIndexV1 { pub available: u64, pub index: i64 }

/// Reads a cursor's persistent next index without reserving it or changing PHP-visible storage.
pub type MbQueryNextV1 = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut MbQueryIndexV1) -> i32;

/// Selects a writable nested array and publishes one child pin before retiring replaced owners.
pub type MbQueryEnterV1 = unsafe extern "C" fn(
    *mut c_void, *mut c_void, *const MbHostValueV1, *mut *mut c_void,
) -> i32;

/// Stores a borrowed string value using an already-normalized integer or binary-string key.
pub type MbQueryStoreV1 = unsafe extern "C" fn(
    *mut c_void, *mut c_void, *const MbHostValueV1, *const MbHostValueV1,
) -> i32;

/// Completes removal from a selected root before returning any pending cleanup status.
pub type MbQueryRemoveV1 = unsafe extern "C" fn(*mut c_void, *mut c_void, *const MbHostValueV1) -> i32;

/// Consumes exactly one cursor pin, completing cleanup even when its final release throws.
pub type MbQueryReleaseV1 = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;

/// Complete V1 storage inventory; all callbacks are required and the context is supplied separately.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbQueryStorageV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub root: Option<MbQueryRootV1>,
    pub next: Option<MbQueryNextV1>,
    pub enter: Option<MbQueryEnterV1>,
    pub store: Option<MbQueryStoreV1>,
    pub remove: Option<MbQueryRemoveV1>,
    pub release: Option<MbQueryReleaseV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the complete native table and append-result layout on the supported 64-bit targets.
    #[test]
    fn mbstring_query_storage_layout() {
        assert_eq!(std::mem::size_of::<MbQueryStorageV1>(), 56);
        assert_eq!(std::mem::offset_of!(MbQueryStorageV1, root), 8);
        assert_eq!(std::mem::offset_of!(MbQueryStorageV1, next), 16);
        assert_eq!(std::mem::offset_of!(MbQueryStorageV1, enter), 24);
        assert_eq!(std::mem::offset_of!(MbQueryStorageV1, store), 32);
        assert_eq!(std::mem::offset_of!(MbQueryStorageV1, remove), 40);
        assert_eq!(std::mem::offset_of!(MbQueryStorageV1, release), 48);
        assert_eq!(std::mem::size_of::<MbQueryIndexV1>(), 16);
        assert_eq!(std::mem::offset_of!(MbQueryIndexV1, index), 8);
    }
}
