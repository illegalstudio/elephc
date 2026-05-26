//! Purpose:
//! Emits the `__rt_json_encode_bool`, `__rt_json_encode_true` runtime helper assembly for json encode bool.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - JSON encoders are emitted formatter state machines; escaping, type tags, and buffer growth are observable PHP behavior.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_json_encode_bool` runtime helper.
///
/// Dispatches to `emit_json_encode_bool_linux_x86_64` on x86_64; on other targets
/// emits an ARM64 `cbnz` branch sequence.
///
/// Input:  x0 = bool value (0 or 1)
/// Output: x1 = string ptr, x2 = string len
pub(crate) fn emit_json_encode_bool(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_bool_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_bool ---");
    emitter.label_global("__rt_json_encode_bool");

    emitter.instruction("cbnz x0, __rt_json_encode_true");                      // if true, emit "true"

    // -- false --
    emitter.adrp("x1", "_json_false");                           // load page of "false" string
    emitter.add_lo12("x1", "x1", "_json_false");                     // resolve "false" address
    emitter.instruction("mov x2, #5");                                          // length of "false"
    emitter.instruction("ret");                                                 // return

    // -- true --
    emitter.label("__rt_json_encode_true");
    emitter.adrp("x1", "_json_true");                            // load page of "true" string
    emitter.add_lo12("x1", "x1", "_json_true");                      // resolve "true" address
    emitter.instruction("mov x2, #4");                                          // length of "true"
    emitter.instruction("ret");                                                 // return
}

/// Emits the x86_64 Linux implementation of `__rt_json_encode_bool`.
///
/// Uses the System V AMD64 ABI: boolean in `rax`, result string in `rax` (ptr) and `rdx` (len).
///
/// Input:  rax = bool value (0 or 1)
/// Output: rax = string ptr, rdx = string len
fn emit_json_encode_bool_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_bool ---");
    emitter.label_global("__rt_json_encode_bool");

    emitter.instruction("test rax, rax");                                       // check whether the incoming SysV boolean payload is truthy
    emitter.instruction("jnz __rt_json_encode_true");                           // non-zero booleans encode as the JSON literal true
    emitter.instruction("lea rax, [rip + _json_false]");                        // materialize the address of the static JSON false literal
    emitter.instruction("mov rdx, 5");                                          // return the byte length of the JSON false literal
    emitter.instruction("ret");                                                 // return the borrowed JSON literal slice in the x86_64 string result registers

    emitter.label("__rt_json_encode_true");
    emitter.instruction("lea rax, [rip + _json_true]");                         // materialize the address of the static JSON true literal
    emitter.instruction("mov rdx, 4");                                          // return the byte length of the JSON true literal
    emitter.instruction("ret");                                                 // return the borrowed JSON literal slice in the x86_64 string result registers
}
