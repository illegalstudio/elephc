use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

mod aarch64;
mod x86_64;

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
        return;
    }

    aarch64::emit(emitter);
}
