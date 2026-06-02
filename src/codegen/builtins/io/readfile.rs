//! Purpose:
//! Emits PHP `readfile` builtin calls.
//! Streams a path to stdout through the runtime helper and returns bytes copied.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returns a boxed `int|false` so byte counts, including `0` and read-error
//!   `-1`, stay distinguishable from an open failure.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `readfile($path)` builtin call.
///
/// Arguments:
/// - `args[0]` must be a path expression (string).
///
/// Behavior:
/// - Calls `__rt_readfile` runtime helper which opens the path and streams its
///   contents to stdout. The raw byte count is returned in the standard return
///   register.
/// - Boxes the result into a `PhpType::Mixed`: `-1` (open failure) → PHP `false`,
///   any `>= 0` byte count → PHP `int` (including `0` for empty files).
///
/// Returns:
/// - Always returns `Some(PhpType::Mixed)` since readfile always produces a
///   boxed value regardless of success or failure.
///
/// A `scheme://...` path whose scheme matches a registered userspace wrapper is
/// routed through `__rt_readfile_wrapper` (fopen + fpassthru + close); any other
/// path uses `__rt_readfile` (raw open + stream, which preserves the read-error
/// `-1` semantics for e.g. directories). Both return the count / `-2` convention
/// that `box_readfile_result` boxes into `int` / `false`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("readfile()");
    emit_expr(&args[0], emitter, ctx, data);
    let wrapper = ctx.next_label("readfile_wrapper");
    let after = ctx.next_label("readfile_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- path string: x1 = ptr, x2 = len --
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0] path ptr, [sp,#8] path len
            emitter.instruction("str x1, [sp, #0]");                            // preserve path ptr across the wrapper-scheme probe
            emitter.instruction("str x2, [sp, #8]");                            // preserve path len across the wrapper-scheme probe
            emitter.instruction("mov x0, x1");                                  // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // x0 = 1 when the scheme matches a registered wrapper
            emitter.instruction("ldr x1, [sp, #0]");                            // restore path ptr for the chosen readfile helper
            emitter.instruction("ldr x2, [sp, #8]");                            // restore path len for the chosen readfile helper
            emitter.instruction(&format!("cbnz x0, {}", wrapper));              // registered wrapper scheme → wrapper readfile path
            abi::emit_call_label(emitter, "__rt_readfile");                     // normal path: raw open + stream to stdout
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            abi::emit_call_label(emitter, "__rt_readfile_wrapper");             // wrapper path: fopen + fpassthru + close
            emitter.label(&after);
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
        }
        Arch::X86_64 => {
            // -- path string: rax = ptr, rdx = len --
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0] path ptr, [rsp+8] path len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // preserve path ptr across the wrapper-scheme probe
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // preserve path len across the wrapper-scheme probe
            emitter.instruction("mov rdi, rax");                                // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov rsi, rdx");                                // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // rax = 1 when the scheme matches a registered wrapper
            emitter.instruction("test rax, rax");                               // matched a registered wrapper scheme?
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore path ptr for the chosen readfile helper
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore path len for the chosen readfile helper
            emitter.instruction(&format!("jnz {}", wrapper));                   // registered wrapper scheme → wrapper readfile path
            abi::emit_call_label(emitter, "__rt_readfile");                     // normal path: raw open + stream to stdout
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            abi::emit_call_label(emitter, "__rt_readfile_wrapper");             // wrapper path: fopen + fpassthru + close
            emitter.label(&after);
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
        }
    }
    box_readfile_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the raw readfile return value into a PHP `Mixed` value.
///
/// Input (register convention):
/// - AArch64: byte count in `x0`, where `-2` indicates open failure.
/// - x86_64: byte count in `rax`, where `-2` indicates open failure.
///
/// Output (ABI):
/// - `x0` / `rax`: boxed `Mixed` — `int` for byte counts `>= 0`, `false` for `-2`.
///
/// For AArch64, uses `x9` as a scratch register for the sentinel comparison.
fn box_readfile_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("readfile_false");
    let done_label = ctx.next_label("readfile_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, #-2");                                 // runtime sentinel -2 means the file could not be opened
            emitter.instruction("cmp x0, x9");                                  // did readfile() fail before streaming began?
            emitter.instruction(&format!("b.eq {}", false_label));              // box PHP false for open failure
            emitter.instruction("mov x1, x0");                                  // move the byte count into the mixed integer payload
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads do not use a high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful byte count, including zero for empty files
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for readfile() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible readfile() failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -2");                                 // runtime sentinel -2 means the file could not be opened
            emitter.instruction(&format!("je {}", false_label));                // box PHP false for open failure
            emitter.instruction("mov rdi, rax");                                // move the byte count into the mixed integer payload
            emitter.instruction("xor esi, esi");                                // integer mixed payloads do not use a high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful byte count, including zero for empty files
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for readfile() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible readfile() failure semantics
            emitter.label(&done_label);
        }
    }
}
