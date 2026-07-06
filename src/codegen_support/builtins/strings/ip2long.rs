//! Purpose:
//! Emits PHP `ip2long` calls.
//! Parses a dotted-quad IPv4 string into an integer, or PHP false when invalid.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - The `__rt_ip2long` helper returns -1 for an invalid address; that case is
//!   boxed as PHP false, a valid result as a boxed integer.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `ip2long()` string builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ip2long()");
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
    abi::emit_call_label(emitter, "__rt_ip2long");
    box_ip2long_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a -1 sentinel becomes PHP `false`, any other value
/// becomes a boxed integer.
fn box_ip2long_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("ip2long_false");
    let done_label = ctx.next_label("ip2long_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the helper report an invalid address?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false on the -1 sentinel
            emitter.instruction("mov x1, x0");                                  // move the parsed integer into the mixed payload
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads have no high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after a valid parse
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the helper report an invalid address?
            emitter.instruction(&format!("js {}", false_label));                // box PHP false on the -1 sentinel
            emitter.instruction("mov rdi, rax");                                // move the parsed integer into the mixed payload
            emitter.instruction("xor esi, esi");                                // integer mixed payloads have no high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = integer
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after a valid parse
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
