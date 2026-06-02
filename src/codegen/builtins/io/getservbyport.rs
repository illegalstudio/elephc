//! Purpose:
//! Emits PHP `getservbyport` calls.
//! Looks up an internet service name by port number and protocol.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_getservbyport` helper returns a null pointer when no entry
//!   matches; that case is boxed as PHP false, a found name as a boxed string.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("getservbyport()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x0, x0, [sp, #-16]!");                     // push the port number while the protocol string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldp x0, x9, [sp], #16");                       // restore the port number into the first helper argument
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rax");                     // push the port number while the protocol string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rsi, rax");                                // protocol pointer becomes the second SysV helper argument
            abi::emit_pop_reg_pair(emitter, "rdi", "rcx");                      // restore the port number into the first SysV helper argument
        }
    }
    abi::emit_call_label(emitter, "__rt_getservbyport");
    box_string_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a null pointer becomes PHP `false`, a non-null
/// pointer/length pair becomes a boxed string without copying the buffer.
fn box_string_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("getservbyport_false");
    let done_label = ctx.next_label("getservbyport_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null pointer means no entry matched
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the string payload across the allocation
            emitter.instruction("mov x0, #24");                                 // mixed cells store a tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation as a mixed cell
            emitter.instruction("mov x9, #1");                                  // runtime tag 1 = string
            emitter.instruction("str x9, [x0]");                                // store the string tag
            abi::emit_pop_reg_pair(emitter, "x10", "x11"); // reload the string pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store the string payload words
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after a valid result
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // a null pointer means no entry matched
            emitter.instruction(&format!("jz {}", false_label));                // box false when no entry matched
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the string payload across the allocation
            emitter.instruction("mov rax, 24");                                 // mixed cells store a tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                (X86_64_HEAP_MAGIC_HI32 << 32) | 5
            )); // mixed-cell heap-kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as a mixed cell
            emitter.instruction("mov r10, 1");                                  // runtime tag 1 = string
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the string tag
            abi::emit_pop_reg_pair(emitter, "r10", "r11"); // reload the string pointer and length
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the string pointer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // store the string length
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after a valid result
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
