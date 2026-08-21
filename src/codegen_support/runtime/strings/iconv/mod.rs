//! Purpose:
//! Groups the runtime helpers PHP's iconv builtins reach at run time.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the string runtime registry.
//!
//! Key details:
//! - `call` owns the bridge call, the diagnostic, and the result-block lifetime.
//! - `options` reads `iconv_mime_encode()`'s option array at the call site.
//! - `result` owns turning one result block into the PHP value the caller stores.
//! - Both are emitted for every program; the bridge itself is only linked when a lowered
//!   iconv builtin publishes its function-pointer slots.

mod call;
mod options;
mod result;

use crate::codegen_support::emit::Emitter;

/// Emits every iconv runtime helper for the active target.
pub fn emit_iconv(emitter: &mut Emitter) {
    call::emit_iconv_call(emitter);
    options::emit_iconv_mime_option(emitter);
    result::emit_iconv_result(emitter);
}
