//! Purpose:
//! Defines protected callable resolution and invocation for shared mbregex replacements.
//!
//! Called from:
//! - The mbstring callback coordinator and native/eval runtime host adapters.
//!
//! Key details:
//! - Statuses use RuntimeBuiltinStatus; no callback may unwind through the engine.
//! - Published boxes are consumed by the accompanying value host, including on failure.

use std::ffi::c_void;
use super::invoke::MbCloneValueV1;

/// One invocation borrows the encoded capture graph and transfers an owned boxed PHP result.
/// The host treats graph fields as immutable and initializes no other engine-owned storage.
/// A published result must be released even when invocation returns a failure status.
#[repr(C)]
pub struct MbCallbackCallV1 {
    pub captures: *const u8,
    pub captures_len: u64,
    pub result: *mut c_void,
}

/// Calls a previously resolved callback with one capture-array argument and returns its boxed value.
/// The callback, graph, and request remain borrowed until return. The result is independently
/// owned, including when PHP returns its argument; the value host performs the later string cast.
pub type MbCallbackInvokeV1 = unsafe extern "C" fn(*mut c_void, *const c_void, *mut MbCallbackCallV1) -> i32;

/// Complete V1 replacement host, separate from the ordinary value and output-reference protocols.
/// Resolution validates argument two of mb_ereg_replace_callback and returns an independently
/// owned callable box, or publishes its PHP TypeError and returns PendingThrowable.
/// Both callbacks are required; their boxes obey the accompanying value host's ownership contract.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbCallbackHostV1 {
    pub version: u32,
    pub size: u32,
    pub context: *mut c_void,
    pub resolve: Option<MbCloneValueV1>,
    pub invoke: Option<MbCallbackInvokeV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the complete callback records used by native emitters on every supported target.
    #[test]
    fn mbstring_callback_host_layout() {
        assert_eq!(std::mem::size_of::<MbCallbackHostV1>(), 32);
        assert_eq!(std::mem::offset_of!(MbCallbackHostV1, version), 0);
        assert_eq!(std::mem::offset_of!(MbCallbackHostV1, size), 4);
        assert_eq!(std::mem::offset_of!(MbCallbackHostV1, context), 8);
        assert_eq!(std::mem::offset_of!(MbCallbackHostV1, resolve), 16);
        assert_eq!(std::mem::offset_of!(MbCallbackHostV1, invoke), 24);
        assert_eq!(std::mem::size_of::<MbCallbackCallV1>(), 24);
        assert_eq!(std::mem::offset_of!(MbCallbackCallV1, captures), 0);
        assert_eq!(std::mem::offset_of!(MbCallbackCallV1, captures_len), 8);
        assert_eq!(std::mem::offset_of!(MbCallbackCallV1, result), 16);
    }
}
