//! Purpose:
//! Emits PHP `stream_context_get_options` calls. Returns the global
//! `_stream_context_options` hash (set by `stream_context_create` /
//! `stream_context_set_option`) so callers can inspect the persisted
//! options. When no context has been created yet, returns an empty hash.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The context argument is evaluated for its side effects but ignored:
//!   v1 keeps a single global context, so every `$ctx` resolves to the
//!   same hash.
//! - The returned pointer is the same hash stored globally — callers
//!   should not free it.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_get_options()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_get_options()");
    // Evaluate the context for its side effects; v1 keeps one global context.
    emit_expr(&args[0], emitter, ctx, data);
    let empty_label = ctx.next_label("scgo_empty");
    let done_label = ctx.next_label("scgo_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
            emitter.instruction("ldr x0, [x9]");                                // load the persisted hash pointer
            emitter.instruction(&format!("cbz x0, {}", empty_label));           // no context yet → return an empty hash
            emitter.instruction("bl __rt_incref");                              // hand the caller a retained reference
            emitter.instruction(&format!("b {}", done_label));                  // continue at target label
            emitter.label(&empty_label);
            emitter.instruction("mov x0, #1");                                  // initial capacity for the empty fallback hash
            emitter.instruction("mov x1, #7");                                  // value type tag = Mixed
            abi::emit_call_label(emitter, "__rt_hash_new");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r9", "_stream_context_options"); // load runtime data address
            emitter.instruction("mov rax, QWORD PTR [r9]");                     // load the persisted hash pointer
            emitter.instruction("test rax, rax");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", empty_label));                // empty fallback when no context exists
            emitter.instruction("mov rdi, rax");                                // incref's SysV arg
            emitter.instruction("call __rt_incref");                            // hand the caller a retained reference
            emitter.instruction(&format!("jmp {}", done_label));                // continue at target label
            emitter.label(&empty_label);
            emitter.instruction("mov edi, 1");                                  // initial capacity
            emitter.instruction("mov esi, 7");                                  // value type tag = Mixed
            abi::emit_call_label(emitter, "__rt_hash_new");
            emitter.label(&done_label);
        }
    }
    Some(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}
