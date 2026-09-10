//! Purpose:
//! Defines private C requests for eval output operations and callback exception containment.
//!
//! Called from:
//! - Magician's runtime value adapter and the compiler's protected output emitter.
//!
//! Key details:
//! - Inputs are borrowed for one call; successful get-and-pop results transfer owned native bytes.
//! - The exported entry returns zero on success, one on invalid action, or two for a pending Throwable.

/// Stable action numbers for the version-one protected output entry.
#[derive(Clone, Copy)]
#[repr(u64)]
pub enum OutputAction {
    Echo = 0,
    Start = 1,
    Clean = 2,
    Flush = 3,
    End = 4,
    GetEnd = 5,
}

/// Borrowed input words and independently published output storage for one protected operation.
#[repr(C)]
pub struct OutputRequestV1 {
    pub action: u64,
    pub arguments: [u64; 6],
    pub result: i64,
    pub bytes: *const u8,
    pub length: i64,
}

impl OutputRequestV1 {
    /// Initializes one request with no transferred output ownership.
    pub const fn new(action: OutputAction, arguments: [u64; 6]) -> Self {
        Self { action: action as u64, arguments, result: 0, bytes: std::ptr::null(), length: 0 }
    }
}

/// One borrowed handler invocation with separate owned result and Throwable outputs.
/// The callback returns zero for success, one for a fatal error, or two for a PHP throw.
#[repr(C)]
pub struct OutputHandlerCallV1 {
    pub id: u64,
    pub bytes: *const u8,
    pub length: u64,
    pub phase: i64,
    pub result: *mut std::ffi::c_void,
    pub thrown: *mut std::ffi::c_void,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the C layout consumed by both supported native architectures.
    #[test]
    fn output_request_layout_matches_native_emission() {
        assert_eq!(std::mem::size_of::<OutputRequestV1>(), 80);
        assert_eq!(std::mem::offset_of!(OutputRequestV1, arguments), 8);
        assert_eq!(std::mem::offset_of!(OutputRequestV1, result), 56);
        assert_eq!(std::mem::offset_of!(OutputRequestV1, bytes), 64);
        assert_eq!(std::mem::offset_of!(OutputRequestV1, length), 72);
    }

    /// Pins the callback record marshalled by native output-handler trampolines.
    #[test]
    fn output_handler_call_layout_matches_native_emission() {
        assert_eq!(std::mem::size_of::<OutputHandlerCallV1>(), 48);
        assert_eq!(std::mem::offset_of!(OutputHandlerCallV1, bytes), 8);
        assert_eq!(std::mem::offset_of!(OutputHandlerCallV1, length), 16);
        assert_eq!(std::mem::offset_of!(OutputHandlerCallV1, phase), 24);
        assert_eq!(std::mem::offset_of!(OutputHandlerCallV1, result), 32);
        assert_eq!(std::mem::offset_of!(OutputHandlerCallV1, thrown), 40);
    }
}
