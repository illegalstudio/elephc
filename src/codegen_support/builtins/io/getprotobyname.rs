//! Purpose:
//! Emits PHP `getprotobyname` calls.
//! Looks up a protocol number by name or alias in `/etc/protocols`.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_getprotobyname` helper returns -1 when no entry matches; that
//!   case is boxed as PHP false, a valid number as a boxed integer.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `getprotobyname()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("getprotobyname()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // string pointer becomes the first helper argument
            emitter.instruction("mov x1, x2");                                  // string length becomes the second helper argument
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // string pointer becomes the first SysV argument
            emitter.instruction("mov rsi, rdx");                                // string length becomes the second SysV argument
        }
    }
    abi::emit_call_label(emitter, "__rt_getprotobyname");
    box_protocol_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a -1 sentinel becomes PHP `false`, any other value
/// becomes a boxed integer.
fn box_protocol_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("getprotobyname_false");
    let done_label = ctx.next_label("getprotobyname_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the helper find no matching entry?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false on the -1 sentinel
            emitter.instruction("mov x1, x0");                                  // move the protocol number into the mixed payload
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads have no high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after a valid lookup
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the helper find no matching entry?
            emitter.instruction(&format!("js {}", false_label));                // box PHP false on the -1 sentinel
            emitter.instruction("mov rdi, rax");                                // move the protocol number into the mixed payload
            emitter.instruction("xor esi, esi");                                // integer mixed payloads have no high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = integer
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after a valid lookup
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
