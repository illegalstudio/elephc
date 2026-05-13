//! Purpose:
//! Emits the `__rt_json_encode_array_str`, `__rt_json_arr_str_loop` runtime helper assembly for json encode array str.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - JSON encoders are emitted formatter state machines; escaping, type tags, and buffer growth are observable PHP behavior.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_array_str: encode a string array as JSON '["a","b"]'.
///
/// Tail-calls `__rt_json_encode_array_dynamic`, which dispatches every
/// element through `__rt_json_encode_str`. The previous inlined fast path
/// only handled `\"`, `\\`, and `\n` and emitted raw bytes for tabs,
/// carriage returns, backspace, form-feed, the rest of the 0x00..0x1F
/// control range, and multibyte UTF-8 — producing invalid JSON. Every
/// encoder flag (`JSON_FORCE_OBJECT`, `JSON_NUMERIC_CHECK`,
/// `JSON_INVALID_UTF8_IGNORE`, `JSON_INVALID_UTF8_SUBSTITUTE`,
/// `JSON_HEX_*`, `JSON_UNESCAPED_*`) is also honored by routing through
/// the dynamic encoder.
///
/// Input:  x0 = array pointer (header: len[8], cap[8], then pairs of
///         ptr[8]+len[8])
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub(crate) fn emit_json_encode_array_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_array_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_str ---");
    emitter.label_global("__rt_json_encode_array_str");

    emitter.instruction("b __rt_json_encode_array_dynamic");                    // tail-call the dispatcher-aware encoder so every element flows through __rt_json_encode_str
}

fn emit_json_encode_array_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_str ---");
    emitter.label_global("__rt_json_encode_array_str");

    emitter.instruction("jmp __rt_json_encode_array_dynamic");                  // tail-call the dispatcher-aware encoder so every element flows through __rt_json_encode_str
}
