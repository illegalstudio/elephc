//! Purpose:
//! Emits PHP `stream_socket_pair` calls.
//! Creates a connected pair of sockets and yields them as a two-element array.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Marshals the domain, type, and protocol into the three
//!   `__rt_stream_socket_pair` argument registers; the helper returns the
//!   pointer to a freshly built indexed array of socket resources.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_pair()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the domain
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the type
    emit_expr(&args[2], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x2, x0");                                  // protocol into argument 2
            abi::emit_pop_reg(emitter, "x1"); // type into argument 1
            abi::emit_pop_reg(emitter, "x0"); // domain into argument 0
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // protocol into argument 2
            abi::emit_pop_reg(emitter, "rsi"); // type into argument 1
            abi::emit_pop_reg(emitter, "rdi"); // domain into argument 0
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_socket_pair");
    box_pair_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Widens the descriptor array's typed int slots into boxed Mixed(resource)
/// cells, then boxes the resulting array pointer as a Mixed indexed-array
/// cell. A null pointer from the helper (socketpair failure) lowers to a
/// Mixed false cell instead.
fn box_pair_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("ssp_false");
    let done_label = ctx.next_label("ssp_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", false_label));           // null pointer => box PHP false
            emitter.instruction("mov x1, #9");                                  // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(emitter, "__rt_array_to_mixed");               // widen slots from raw ints to boxed Mixed pointers
            emitter.instruction("mov x1, x0");                                  // success: converted array pointer becomes the Mixed payload low word
            emitter.instruction("mov x2, #0");                                  // indexed array mixed payloads do not use a high word
            emitter.instruction("mov x0, #4");                                  // runtime tag 4 = indexed array
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the success array as a Mixed cell
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // bool payload = 0 for false
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box PHP false for socketpair() failure
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null pointer signals socketpair failure
            emitter.instruction(&format!("jz {}", false_label));                // box PHP false when socketpair() failed
            emitter.instruction("mov rdi, rax");                                // array pointer for __rt_array_to_mixed
            emitter.instruction("mov esi, 9");                                  // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(emitter, "__rt_array_to_mixed");               // widen slots from raw ints to boxed Mixed pointers
            emitter.instruction("mov rdi, rax");                                // converted array pointer becomes the Mixed payload low word
            emitter.instruction("xor esi, esi");                                // indexed array mixed payloads do not use a high word
            emitter.instruction("mov eax, 4");                                  // runtime tag 4 = indexed array
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the success array as a Mixed cell
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // bool payload = 0 for false
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box PHP false for socketpair() failure
            emitter.label(&done_label);
        }
    }
}
