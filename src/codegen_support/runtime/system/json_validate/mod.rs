//! Purpose:
//! Emits target dispatch for JSON validator runtime helpers.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - The exported validator must share JSON state slots with decode and encode entry points.
//! - Public decode and validate calls publish their original source slice.
//!   Errors materialize coordinates once from `__rt_json_throw_error`; empty
//!   input has no location. String failures point at the token opener.
//! - Token coordinates follow PHP 8.6, extending PHP 8.2-8.5 base messages.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

mod aarch64;
mod diagnostics;
mod x86_64;
#[cfg(test)]
mod tests;

/// __rt_json_validate(json_ptr, json_len) -> bool
///
/// Recursive-descent JSON validator implementing the RFC 8259 grammar:
/// literals (`null` / `true` / `false`), numbers (`-?(0|[1-9][0-9]*)
/// (.[0-9]+)?([eE][+-]?[0-9]+)?`), strings with escape validation
/// (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uHHHH`), arrays,
/// and objects. Recursion depth is enforced against `_json_depth_limit`,
/// and on overflow `JSON_ERROR_DEPTH` is recorded; on a malformed token
/// `JSON_ERROR_SYNTAX` is recorded. Both error paths route through
/// `__rt_json_throw_error`, which raises `JsonException` whenever
/// `JSON_THROW_ON_ERROR` is set in `_json_active_flags`.
///
/// State globals (single-threaded; helpers call each other recursively
/// so the source slice is parked in BSS rather than threaded through the
/// stack frames):
///   _json_validate_ptr   : source pointer
///   _json_validate_len   : source length
///   _json_validate_idx   : current source index (0-based)
///   _json_active_depth   : current nesting depth
///   _json_depth_limit    : maximum allowed nesting depth
///
/// Input ABI:
///   ARM64: x1 = json ptr, x2 = json len
///   x86_64: rax = json ptr, rdx = json len
/// Output: x0/rax = 1 on success, 0 on failure.
pub(crate) fn emit_json_validate(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit(emitter);
    } else {
        aarch64::emit(emitter);
    }
    diagnostics::emit_decode_syntax_error(emitter);
}
