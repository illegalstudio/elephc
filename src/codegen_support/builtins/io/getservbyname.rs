//! Purpose:
//! Emits PHP `getservbyname` calls.
//! Looks up an internet service port by service name and protocol.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_getservbyname` helper returns -1 when no entry matches; that
//!   case is boxed as PHP false, a valid port as a boxed integer.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `getservbyname()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("getservbyname()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the service string while the protocol string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // protocol pointer becomes the third helper argument
            emitter.instruction("mov x4, x2");                                  // protocol length becomes the fourth helper argument
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the service string into the first two helper arguments
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the service string while the protocol string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // protocol length becomes the fourth SysV helper argument
            emitter.instruction("mov rdx, rax");                                // protocol pointer becomes the third SysV helper argument
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the service string into the first two SysV helper arguments
        }
    }
    abi::emit_call_label(emitter, "__rt_getservbyname");
    box_port_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a -1 sentinel becomes PHP `false`, any other value
/// becomes a boxed integer.
fn box_port_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("getservbyname_false");
    let done_label = ctx.next_label("getservbyname_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the helper find no matching entry?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false on the -1 sentinel
            emitter.instruction("mov x1, x0");                                  // move the service port into the mixed payload
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
            emitter.instruction("mov rdi, rax");                                // move the service port into the mixed payload
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
