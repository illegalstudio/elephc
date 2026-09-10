//! Purpose:
//! Selects PHP bytewise case conversion in the shared ASCII string runtime.
//!
//! Called from:
//! - The runtime emitter for every supported target.
//!
//! Key details:
//! - Unchanged values preserve logical identity; changed values have fresh origins and owned bytes.

use crate::codegen_support::emit::Emitter;

/// Emits case conversion with optional native INI origin tracking.
pub fn emit_strtoupper(emitter: &mut Emitter, mbstring: bool) {
    super::ascii_case::emit(emitter, true, mbstring);
}
