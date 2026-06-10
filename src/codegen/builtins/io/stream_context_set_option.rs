//! Purpose:
//! Emits PHP `stream_context_set_option` calls.
//!
//! - 2-arg form `stream_context_set_option($ctx, array $options)`:
//!   replaces the global `_stream_context_options` hash with the new
//!   options array, same persistence semantics as
//!   `stream_context_create`.
//! - 4-arg form `stream_context_set_option($ctx, $wrapper, $option, $value)`:
//!   v1 stub — evaluates all arguments for side effects, reports `true`,
//!   but the single-option update is not yet propagated into the global
//!   hash. Full nested-hash mutation is deferred (the runtime support
//!   would need __rt_hash_get + __rt_hash_set chained across the
//!   wrapper sub-hash).
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The retained options hash is `__rt_incref`'d so the global slot
//!   outlives the temporary owner produced by the caller's array
//!   literal.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_set_option()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_set_option()");
    // -- evaluate the context arg (just for side effects: v1 has one global slot) --
    emit_expr(&args[0], emitter, ctx, data);

    if args.len() == 2 {
        // 2-arg form: replace the global options hash with the new array.
        emit_expr(&args[1], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                let store_done = ctx.next_label("scso_store_done");
                emitter.instruction(&format!("cbz x0, {}", store_done));        // null options → leave the slot unchanged
                abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
                emitter.instruction("str x0, [x9]");                            // overwrite the persisted options
                emitter.instruction("bl __rt_incref");                          // retain the new hash so the global slot owns it
                emitter.label(&store_done);
                emitter.instruction("mov x0, #1");                              // PHP true
            }
            Arch::X86_64 => {
                let store_done = ctx.next_label("scso_store_done_x86");
                emitter.instruction("test rax, rax");                           // check whether the runtime value is zero
                emitter.instruction(&format!("jz {}", store_done));             // null options → leave the slot unchanged
                abi::emit_symbol_address(emitter, "r9", "_stream_context_options"); // load runtime data address
                emitter.instruction("mov QWORD PTR [r9], rax");                 // overwrite the persisted options
                emitter.instruction("mov rdi, rax");                            // incref's first arg
                emitter.instruction("call __rt_incref");                        // call runtime helper
                emitter.label(&store_done);
                emitter.instruction("mov eax, 1");                              // PHP true
            }
        }
    } else if args.len() == 4 {
        // 4-arg form: stream_context_set_option($ctx, $wrapper, $opt, $value)
        // Marshals the three strings into the runtime helper which navigates
        // the nested options[wrapper][option] = value structure.
        emit_expr(&args[1], emitter, ctx, data);                                // wrapper string in result regs
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg_pair(emitter, "x1", "x2");                   // preserve wrapper ptr/len
                emit_expr(&args[2], emitter, ctx, data);                        // option string
                abi::emit_push_reg_pair(emitter, "x1", "x2");                   // preserve option ptr/len
                emit_expr(&args[3], emitter, ctx, data);                        // value string
                emitter.instruction("mov x4, x1");                              // value_ptr → 5th arg
                emitter.instruction("mov x5, x2");                              // value_len → 6th arg
                abi::emit_pop_reg_pair(emitter, "x2", "x3");                    // restore option ptr/len → 3rd/4th args
                abi::emit_pop_reg_pair(emitter, "x0", "x1");                    // restore wrapper ptr/len → 1st/2nd args
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve wrapper ptr/len
                emit_expr(&args[2], emitter, ctx, data);                        // option string
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve option ptr/len
                emit_expr(&args[3], emitter, ctx, data);                        // value string
                emitter.instruction("mov r8, rax");                             // value_ptr → 5th arg
                emitter.instruction("mov r9, rdx");                             // value_len → 6th arg
                abi::emit_pop_reg_pair(emitter, "rdx", "rcx");                  // restore option ptr/len → 3rd/4th args
                abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                  // restore wrapper ptr/len → 1st/2nd args
            }
        }
        abi::emit_call_label(emitter, "__rt_stream_context_set_option_4");
    } else {
        // Other arities (shouldn't reach here after the checker accepts only
        // 2 or 4) — evaluate side effects and report success.
        for arg in &args[1..] {
            emit_expr(arg, emitter, ctx, data);
        }
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #1"),                 // prepare AArch64 call argument
            Arch::X86_64 => emitter.instruction("mov eax, 1"),                  // prepare runtime result value
        }
    }
    Some(PhpType::Bool)
}
