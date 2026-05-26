//! Purpose:
//! Emits the `__rt_json_encode_null` runtime helper assembly for json encode null.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - JSON encoders are emitted formatter state machines; escaping, type tags, and buffer growth are observable PHP behavior.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_json_encode_null` runtime helper.
///
/// dispatches to the target-specific emitter. On ARM64 the result registers are
/// `x1` (string ptr) and `x2` (string len). On x86_64 the result registers are
/// `rax` (string ptr) and `rdx` (string len).
pub(crate) fn emit_json_encode_null(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_null_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_null ---");
    emitter.label_global("__rt_json_encode_null");

    emitter.adrp("x1", "_json_null");                            // load page of "null" string
    emitter.add_lo12("x1", "x1", "_json_null");                      // resolve "null" address
    emitter.instruction("mov x2, #4");                                          // length of "null"
    emitter.instruction("ret");                                                 // return
}

/// Emits the `__rt_json_encode_null` runtime helper for the x86_64 Linux ABI.
/// Returns the address of the static `"null"` literal in `rax` and its byte length (4) in `rdx`.
fn emit_json_encode_null_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_null ---");
    emitter.label_global("__rt_json_encode_null");

    emitter.instruction("lea rax, [rip + _json_null]");                         // materialize the address of the static JSON null literal
    emitter.instruction("mov rdx, 4");                                          // return the byte length of the JSON null literal
    emitter.instruction("ret");                                                 // return the borrowed JSON literal slice in the x86_64 string result registers
}
