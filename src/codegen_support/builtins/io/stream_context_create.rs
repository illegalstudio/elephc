//! Purpose:
//! Emits PHP `stream_context_create` calls. Persists the options hash in
//! the runtime's `_stream_context_options` slot so
//! `stream_context_get_options` / `stream_context_set_option` and future
//! consumer integrations (http://, ftp://, fopen's 4th-arg context) can
//! read it back.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - v1 limitation: only one active context at a time — every
//!   stream_context_create call overwrites the previous options slot.
//!   Per-resource contexts would need a registry indexed by the synthetic
//!   context fd; deferred until any real use case needs it.
//! - The options hash is `__rt_incref`'d before being saved so the global
//!   slot survives the surrounding owner's scope-exit decref.
//! - Returns a non-zero synthetic resource id so `is_resource()` and
//!   `gettype()` keep working as before.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_create()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_create()");
    if !args.is_empty() {
        // -- evaluate the options array, retain it, and stash it globally --
        emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                // Unique labels: a program may call stream_context_create more
                // than once, so a fixed label name would be defined twice and
                // fail to assemble.
                let store_zero = ctx.next_label("scc_store_zero");
                let store_done = ctx.next_label("scc_store_done");
                emitter.instruction(&format!("cbz x0, {}", store_zero));        // null option ptr → store null without incref
                abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
                emitter.instruction("str x0, [x9]");                            // _stream_context_options = options hash
                emitter.instruction("bl __rt_incref");                          // retain the hash so the global slot owns it
                emitter.instruction(&format!("b {}", store_done));              // continue at target label
                emitter.label(&store_zero);
                abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
                emitter.instruction("str xzr, [x9]");                           // clear the slot when no options were passed
                emitter.label(&store_done);
            }
            Arch::X86_64 => {
                // Unique labels: see the AArch64 note above.
                let store_zero = ctx.next_label("scc_store_zero_x86");
                let store_done = ctx.next_label("scc_store_done_x86");
                emitter.instruction("test rax, rax");                           // check whether the runtime value is zero
                emitter.instruction(&format!("jz {}", store_zero));             // null options pointer → clear the slot
                abi::emit_symbol_address(emitter, "r9", "_stream_context_options"); // load runtime data address
                emitter.instruction("mov QWORD PTR [r9], rax");                 // _stream_context_options = options hash
                emitter.instruction("mov rdi, rax");                            // incref's SysV arg
                emitter.instruction("call __rt_incref");                        // retain the hash
                emitter.instruction(&format!("jmp {}", store_done));            // continue at target label
                emitter.label(&store_zero);
                abi::emit_symbol_address(emitter, "r9", "_stream_context_options"); // load runtime data address
                emitter.instruction("mov QWORD PTR [r9], 0");                   // clear the slot
                emitter.label(&store_done);
            }
        }
    } else {
        // -- no options arg: leave the slot untouched --
    }
    // -- capture the optional second arg (params) `notification` callback --
    // Evaluates params for side effects and stashes a literal closure /
    // first-class-callable `notification` entry into the global so __rt_http_open
    // can fire it at the STREAM_NOTIFY_* milestones.
    super::stream_notification::capture_notification_callback(args.get(1), emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #1"),                     // synthetic context resource id (1 = the single global context)
        Arch::X86_64 => emitter.instruction("mov eax, 1"),                      // synthetic context resource id
    }
    Some(PhpType::stream_resource())
}
