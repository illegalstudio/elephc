//! Purpose:
//! Emits PHP `flock` advisory-locking builtin calls over runtime file handles.
//! Validates the stream argument before invoking the libc `flock` wrapper.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The runtime translates the PHP `LOCK_UN` value (3) to the POSIX value (8),
//!   preserves `LOCK_NB`, and returns the optional `$would_block` state.

use crate::codegen_support::context::{Context, HeapOwnership};
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code for the PHP `flock(stream, operation, &$would_block?)` builtin.
///
/// Validates the stream argument and extracts its file descriptor. Emits the lock
/// operation expression, then places both fd (in x0/rax) and operation (in x1/rdx)
/// into the standard integer argument registers before calling `__rt_flock`. On return,
/// optionally stores the runtime's `$would_block` output into the caller's variable.
///
/// Returns `PhpType::Bool` unconditionally.
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
    // -- user-wrapper synthetic fd path (G1): dispatch into stream_lock --
    //    A descriptor >= USER_WRAPPER_FD_BASE is a userspace wrapper handle, so
    //    flock() must call the wrapper's stream_lock() rather than the libc
    //    flock() wrapper. PHP does not populate $would_block for userspace
    //    wrappers, so the wrapper path skips the by-ref store entirely.
    let wrapper_label = ctx.next_label("flock_user_wrapper");
    let done_label = ctx.next_label("flock_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // dispatch into the wrapper's stream_lock
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // dispatch into the wrapper's stream_lock
        }
    }
    abi::emit_call_label(emitter, "__rt_flock");                                // call the runtime libc flock(fd, op) wrapper that translates LOCK_UN
    if let Some(would_block_arg) = args.get(2) {
        emit_store_would_block(would_block_arg, emitter, ctx);
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", done_label)),     // skip the wrapper path on the normal-fd result
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", done_label)),    // skip the wrapper path on the normal-fd result
    }
    emitter.label(&wrapper_label);
    // `__rt_user_wrapper_flock` resolves the wrapper object from the synthetic
    // fd and calls stream_lock($operation). Its lookup expects the fd in the
    // SysV first-arg register (x0 / rdi) and the operation in the second (x1 /
    // rsi). ARM64 already holds fd in x0 and operation in x1; x86_64 left fd in
    // rax and operation in rdx (the libc `__rt_flock` convention), so move both
    // into the wrapper-call registers first.
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // synthetic fd → wrapper-lookup first-arg register
        emitter.instruction("mov rsi, rdx");                                    // lock operation → wrapper-call second-arg register
    }
    abi::emit_call_label(emitter, "__rt_user_wrapper_flock");                   // call the wrapper's stream_lock($operation)
    emitter.label(&done_label);
    Some(PhpType::Bool)
}

/// Emits code to store the `$would_block` output from `__rt_flock` into the variable
/// represented by `arg`.
///
/// Uses a push/pop cycle to preserve the `flock()` return value across the store.
/// On ARM64 the runtime writes `would_block` to x1; on x86_64 it writes to rdx.
/// In both cases the value is moved to the standard scalar result register (x0/rax)
/// before calling `emit_store_would_block_result`.
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

/// Stores the `would_block` boolean result into the variable identified by `name`.
///
/// Resolves the variable's storage location:
/// - **Global variable**: uses `__rt_flock`'s page-relative `would_block` output via a global symbol
/// - **Ref parameter** (passed by reference): loads the parameter's stack address and stores through it
/// - **Local stack variable**: stores directly at the variable's stack offset
///
/// Updates the variable's type to `PhpType::Int` (0 or 1) and marks it `NonHeap`.
/// Panics if a ref param lacks a stack slot.
fn emit_store_would_block_result(name: &str, emitter: &mut Emitter, ctx: &mut Context) {
    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x9", &label);                // resolve global would_block storage address
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
