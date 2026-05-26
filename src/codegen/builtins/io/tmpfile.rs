//! Purpose:
//! Emits PHP `tmpfile` builtin calls.
//! Creates an auto-deleting temp file through the runtime helper and boxes the
//! result as a stream resource (or false on failure) for PHP-compatible typing.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The runtime helper returns the raw fd in the result register (or -1 on
//!   failure). The wrapper boxes it through `__rt_mixed_from_value` like
//!   `fopen` so the result type is `resource|false`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `tmpfile` builtin calls.
///
/// Calls `__rt_tmpfile` runtime helper, then boxes the raw fd result (or -1
/// on failure) into a `PhpType::Mixed` value using `__rt_mixed_from_value`.
/// The boxed result is `resource|false` matching PHP's `tmpfile()` signature.
/// Returns `Some(PhpType::Mixed)`.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("tmpfile()");
    abi::emit_call_label(emitter, "__rt_tmpfile");                              // call the runtime helper that creates an auto-deleting /tmp/elephc-XXXXXX file
    box_tmpfile_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the raw fd result from `__rt_tmpfile` into a PHP-compatible `Mixed`.
///
/// On entry the result register holds the file descriptor (>= 0 on success,
/// -1 on failure). This function branches on the result, then calls
/// `__rt_mixed_from_value` to box either a resource (tag 9) or bool false
/// (tag 3) into the Mixed value returned to PHP.
fn box_tmpfile_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("tmpfile_false");
    let done_label = ctx.next_label("tmpfile_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did tmpfile() return a negative descriptor for failure?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false when the temp file could not be created
            emitter.instruction("mov x1, x0");                                  // move the native stream descriptor into the mixed payload low word
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads do not use a high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful stream resource result
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path after a successful tmpfile
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for tmpfile() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible tmpfile() failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did tmpfile() return a negative descriptor for failure?
            emitter.instruction(&format!("js {}", false_label));                // box PHP false when the temp file could not be created
            emitter.instruction("mov rdi, rax");                                // move the native stream descriptor into the mixed payload low word
            emitter.instruction("xor esi, esi");                                // resource mixed payloads do not use a high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful stream resource result
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path after a successful tmpfile
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for tmpfile() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible tmpfile() failure semantics
            emitter.label(&done_label);
        }
    }
}
