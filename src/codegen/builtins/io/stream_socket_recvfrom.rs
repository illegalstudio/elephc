//! Purpose:
//! Emits PHP `stream_socket_recvfrom` calls.
//! Receives a message from a socket and yields it as a `string|false` value.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Marshals the descriptor, length, and optional flags into the three
//!   `__rt_stream_socket_recvfrom` argument registers; omitted flags are 0.
//! - The helper returns an owned heap string, or a null pointer boxed as
//!   PHP false.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_recvfrom()");
    emit_stream_fd_arg("stream_socket_recvfrom", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the descriptor
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the length
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #0"),                 // omitted flags default to 0
            Arch::X86_64 => emitter.instruction("xor eax, eax"),                // omitted flags default to 0
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x2, x0");                                  // receive flags into argument 2
            abi::emit_pop_reg(emitter, "x1"); // length into argument 1
            abi::emit_pop_reg(emitter, "x0"); // descriptor into argument 0
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // receive flags into argument 2
            abi::emit_pop_reg(emitter, "rsi"); // length into argument 1
            abi::emit_pop_reg(emitter, "rdi"); // descriptor into argument 0
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_socket_recvfrom");
    box_string_or_false(emitter, ctx);
    if let Some(addr_arg) = args.get(3) {
        emit_store_recv_address(addr_arg, emitter, ctx);
    }
    Some(PhpType::Mixed)
}

/// Writes the sender address (stashed by `__rt_stream_socket_recvfrom` in the
/// `_recvfrom_addr_*` globals) into the by-reference `$address` variable. The
/// boxed receive result is preserved across the store.
fn emit_store_recv_address(arg: &Expr, emitter: &mut Emitter, ctx: &mut Context) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0"); // preserve the boxed receive result
            abi::emit_symbol_address(emitter, "x9", "_recvfrom_addr_ptr");
            emitter.instruction("ldr x10, [x9]");                               // load the stashed sender address pointer
            abi::emit_symbol_address(emitter, "x9", "_recvfrom_addr_len");
            emitter.instruction("ldr x11, [x9]");                               // load the stashed sender address length
            emit_store_recv_address_slot(name, emitter, ctx);
            abi::emit_pop_reg(emitter, "x0"); // restore the boxed receive result
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax"); // preserve the boxed receive result
            emitter.instruction("lea r9, [rip + _recvfrom_addr_ptr]");          // address of the stashed-pointer global
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // load the stashed sender address pointer
            emitter.instruction("lea r9, [rip + _recvfrom_addr_len]");          // address of the stashed-length global
            emitter.instruction("mov r11, QWORD PTR [r9]");                     // load the stashed sender address length
            emit_store_recv_address_slot(name, emitter, ctx);
            abi::emit_pop_reg(emitter, "rax"); // restore the boxed receive result
        }
    }
    ctx.update_var_type_and_ownership(name, PhpType::Str, HeapOwnership::Owned);
}

/// Stores the address string (pointer in x10/r10, length in x11/r11) into the
/// `$address` variable's 16-byte string slot, dispatching on storage class.
fn emit_store_recv_address_slot(name: &str, emitter: &mut Emitter, ctx: &Context) {
    let is_global =
        ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name));
    if is_global {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.adrp("x9", &label);                                     // load page of the global address variable
                emitter.add_lo12("x9", "x9", &label);                           // resolve the global address variable
                emitter.instruction("str x10, [x9]");                           // store the address string pointer
                emitter.instruction("str x11, [x9, #8]");                       // store the address string length
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, "r10", &label, 0);        // store the address string pointer
                abi::emit_store_reg_to_symbol(emitter, "r11", &label, 8);        // store the address string length
            }
        }
        return;
    }
    if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for recvfrom $address")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offset);                     // load the referenced address storage pointer
                emitter.instruction("str x10, [x9]");                           // store the address string pointer
                emitter.instruction("str x11, [x9, #8]");                       // store the address string length
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r9", offset);                     // load the referenced address storage pointer
                abi::emit_store_to_address(emitter, "r10", "r9", 0);            // store the address string pointer
                abi::emit_store_to_address(emitter, "r11", "r9", 8);            // store the address string length
            }
        }
        return;
    }
    if let Some(offset) = ctx.variables.get(name).map(|var| var.stack_offset) {
        // A local string slot keeps the pointer at `offset` and the length at
        // `offset - 8`, matching `abi::emit_store`/`emit_load` for `PhpType::Str`.
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::store_at_offset(emitter, "x10", offset);                   // store the address string pointer
                abi::store_at_offset(emitter, "x11", offset - 8);               // store the address string length
            }
            Arch::X86_64 => {
                abi::store_at_offset(emitter, "r10", offset);                   // store the address string pointer
                abi::store_at_offset(emitter, "r11", offset - 8);               // store the address string length
            }
        }
    }
}

/// Boxes the helper result: a null pointer becomes PHP `false`, a non-null
/// pointer/length pair becomes a boxed string.
fn box_string_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("stream_socket_recvfrom_false");
    let done_label = ctx.next_label("stream_socket_recvfrom_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null pointer means the receive failed
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
            emitter.instruction("test rax, rax");                               // a null pointer means the receive failed
            emitter.instruction(&format!("jz {}", false_label));                // box false when the receive failed
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
