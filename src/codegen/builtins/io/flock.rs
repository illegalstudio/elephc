//! Purpose:
//! Emits PHP `flock` advisory-locking builtin calls over runtime file handles.
//! Validates the stream argument before invoking the libc `flock` wrapper.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The runtime translates the PHP `LOCK_UN` value (3) to the POSIX value (8),
//!   preserves `LOCK_NB`, and returns the optional `$would_block` state.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("flock()");
    emit_stream_fd_arg("flock", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the operation expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the lock operation into the second runtime argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the primary integer register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // move the lock operation into the secondary x86_64 integer argument register
            abi::emit_pop_reg(emitter, "rax");                                  // restore the file descriptor into the primary integer register
        }
    }
    abi::emit_call_label(emitter, "__rt_flock");                                // call the runtime libc flock(fd, op) wrapper that translates LOCK_UN
    if let Some(would_block_arg) = args.get(2) {
        emit_store_would_block(would_block_arg, emitter, ctx);
    }
    Some(PhpType::Bool)
}

fn emit_store_would_block(arg: &Expr, emitter: &mut Emitter, ctx: &mut Context) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0");                                  // preserve flock() return while storing the by-ref would_block output
            emitter.instruction("mov x0, x1");                                  // move would_block into the standard scalar result register for storage
            emit_store_would_block_result(name, emitter, ctx);
            abi::emit_pop_reg(emitter, "x0");                                   // restore flock() return after updating would_block
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve flock() return while storing the by-ref would_block output
            emitter.instruction("mov rax, rdx");                                // move would_block into the standard scalar result register for storage
            emit_store_would_block_result(name, emitter, ctx);
            abi::emit_pop_reg(emitter, "rax");                                  // restore flock() return after updating would_block
        }
    }
}

fn emit_store_would_block_result(name: &str, emitter: &mut Emitter, ctx: &mut Context) {
    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.adrp("x9", &format!("{}", label));                      // load page of global would_block storage
                emitter.add_lo12("x9", "x9", &format!("{}", label));            // resolve global would_block storage address
                emitter.instruction("str x0, [x9]");                            // store would_block into the global slot
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, "rax", &label, 0);        // store would_block into the global slot
            }
        }
    } else if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for flock() would_block")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offset);                     // load referenced would_block storage address
                emitter.instruction("str x0, [x9]");                            // store would_block through the referenced slot
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r11", offset);                    // load referenced would_block storage address
                abi::emit_store_to_address(emitter, "rax", "r11", 0);           // store would_block through the referenced slot
            }
        }
    } else if let Some(offset) = ctx.variables.get(name).map(|var| var.stack_offset) {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::store_at_offset(emitter, "x0", offset);                    // store would_block in the local variable slot
            }
            Arch::X86_64 => {
                abi::store_at_offset(emitter, "rax", offset);                   // store would_block in the local variable slot
            }
        }
        ctx.update_var_type_and_ownership(name, PhpType::Int, HeapOwnership::NonHeap);
    }
}
