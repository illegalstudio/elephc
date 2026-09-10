//! Purpose:
//! Defines the response metadata and protected header callbacks for mbstring output conversion.
//!
//! Called from:
//! - Native/eval output adapters and the shared output-handler coordinator.
//!
//! Key details:
//! - Metadata reads never execute PHP; header publication contains PHP exceptions.
//! - Each metadata string is borrowed until the next host callback, without an ownership transfer.
//! - An absent MIME pointer differs from a present empty string.

use std::ffi::c_void;

/// Response metadata copied by the coordinator before publishing any header.
/// Null pointers require zero lengths and represent absent settings. Nonnull pointers with
/// zero lengths represent present empty strings. Both flags must be exactly zero or one.
/// MIME bytes may contain NUL; the coordinator applies PHP's C-string boundary.
#[derive(Default)]
#[repr(C)]
pub struct MbOutputInfoV1 {
    pub mimetype: *const u8,
    pub mimetype_len: u64,
    pub default_mimetype: *const u8,
    pub default_mimetype_len: u64,
    pub send_default_content_type: u64,
    pub in_handler: u64,
}

/// Reads actual response metadata without executing PHP or changing request state.
/// Return zero on success or one on native failure; no throwable may be published here.
pub type MbOutputInfoCallbackV1 = unsafe extern "C" fn(*mut c_void, *mut MbOutputInfoV1) -> i32;

/// Publishes one borrowed Content-Type header with replacement disabled.
/// The host updates its response metadata and clears send_default_content_type on success.
/// Ordinary header rejection still returns zero after protected diagnostic delivery.
/// Status one means fatal; two means a contained pending throwable. In the latter case
/// conversion still finishes, including END state reset, before returning that throwable.
/// No callback may unwind through Rust or retain the borrowed header bytes after return.
pub type MbOutputHeaderCallbackV1 = unsafe extern "C" fn(*mut c_void, *const u8, u64) -> i32;

/// Separate response capability, independent of the versioned value-coercion host.
/// Both callbacks are required; version is one and size covers this complete table.
/// The table and context remain immutable and alive until output invocation returns.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbOutputHostV1 {
    pub version: u32,
    pub size: u32,
    pub context: *mut c_void,
    pub info: Option<MbOutputInfoCallbackV1>,
    pub header: Option<MbOutputHeaderCallbackV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins response callback and metadata layouts on every supported 64-bit target.
    #[test]
    fn mbstring_output_host_layout() {
        assert_eq!(std::mem::size_of::<MbOutputHostV1>(), 32);
        assert_eq!(std::mem::offset_of!(MbOutputHostV1, version), 0);
        assert_eq!(std::mem::offset_of!(MbOutputHostV1, size), 4);
        assert_eq!(std::mem::offset_of!(MbOutputHostV1, context), 8);
        assert_eq!(std::mem::offset_of!(MbOutputHostV1, info), 16);
        assert_eq!(std::mem::offset_of!(MbOutputHostV1, header), 24);
        assert_eq!(std::mem::size_of::<MbOutputInfoV1>(), 48);
        assert_eq!(std::mem::offset_of!(MbOutputInfoV1, mimetype), 0);
        assert_eq!(std::mem::offset_of!(MbOutputInfoV1, mimetype_len), 8);
        assert_eq!(std::mem::offset_of!(MbOutputInfoV1, default_mimetype), 16);
        assert_eq!(std::mem::offset_of!(MbOutputInfoV1, default_mimetype_len), 24);
        assert_eq!(std::mem::offset_of!(MbOutputInfoV1, send_default_content_type), 32);
        assert_eq!(std::mem::offset_of!(MbOutputInfoV1, in_handler), 40);
    }
}
