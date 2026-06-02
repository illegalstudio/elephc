//! Purpose:
//! Emits PHP `chmod` filesystem mutation builtin calls.
//! Routes a `scheme://` path matching a registered userspace wrapper to the
//! wrapper's `stream_metadata($path, STREAM_META_ACCESS, $mode)`; all other
//! paths use the libc `__rt_chmod`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.
//! - The wrapper split mirrors `readfile()`: a `__rt_path_is_wrapper` probe picks
//!   the wrapper branch (`__rt_user_wrapper_path_op` with the `stream_metadata`
//!   vtable slot 14, option `STREAM_META_ACCESS` = 6, value = `$mode`) over the
//!   libc filesystem branch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::path_op_wrapper::emit_box_int_as_mixed;

/// `stream_metadata` vtable slot index in the per-class user-wrapper vtable.
const STREAM_METADATA_SLOT: usize = 14;

/// PHP `STREAM_META_ACCESS` option value (`chmod`-style metadata change).
const STREAM_META_ACCESS: usize = 6;

/// Emits the `chmod` builtin call.
///
/// Evaluates the path argument, spills it, evaluates the mode argument, spills
/// it, then probes the path scheme with `__rt_path_is_wrapper`. The wrapper
/// branch calls `__rt_user_wrapper_path_op(path, len, slot=14,
/// option=STREAM_META_ACCESS, value=mode)` which invokes the wrapper's
/// `stream_metadata($path, $option, $value)`; the libc branch calls `__rt_chmod`
/// with the path in `x1`/`x2` (`rax`/`rdx`) and the mode in `x3` (`rdi`).
///
/// Arguments:
/// - `args[0]`: path (string)
/// - `args[1]`: mode (integer octal, e.g. 0o755)
///
/// Returns: `PhpType::Bool` (true on success, false on failure, matching PHP semantics)
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("chmod()");
    emit_expr(&args[0], emitter, ctx, data);
    let wrapper = ctx.next_label("chmod_wrapper");
    let after = ctx.next_label("chmod_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // scratch: [sp,#0] path ptr, [sp,#8] path len, [sp,#16] mode
            emitter.instruction("str x1, [sp, #0]");                            // save the path pointer
            emitter.instruction("str x2, [sp, #8]");                            // save the path length
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #16]");                           // save the mode value
            emitter.instruction("ldr x0, [sp, #0]");                            // path_is_wrapper arg0 = path ptr
            emitter.instruction("ldr x1, [sp, #8]");                            // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // x0 = 1 when the scheme matches a registered wrapper
            emitter.instruction(&format!("cbnz x0, {}", wrapper));              // registered wrapper scheme → wrapper stream_metadata
            emitter.instruction("ldr x1, [sp, #0]");                            // libc path ptr → x1
            emitter.instruction("ldr x2, [sp, #8]");                            // libc path len → x2
            emitter.instruction("ldr x3, [sp, #16]");                           // libc mode → x3
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame before the call (libc helper runs at the original sp)
            abi::emit_call_label(emitter, "__rt_chmod");                        // normal path: libc chmod(path, mode)
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the mode integer
            emit_box_int_as_mixed(emitter);                                     // box $value as mixed → x0 = owned Mixed(int)
            emitter.instruction("str x0, [sp, #16]");                           // stash the boxed value pointer (mode slot reused)
            emitter.instruction("ldr x0, [sp, #0]");                            // wrapper path ptr → x0
            emitter.instruction("ldr x1, [sp, #8]");                            // wrapper path len → x1
            emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // stream_metadata vtable slot
            emitter.instruction(&format!("mov x3, #{}", STREAM_META_ACCESS));   // option = STREAM_META_ACCESS
            emitter.instruction("ldr x4, [sp, #16]");                           // value = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's stream_metadata method
            emitter.instruction("str x0, [sp, #0]");                            // stash the bool result across the value release
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the boxed value pointer
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the boxed $value (caller owns; the method borrowed it)
            emitter.instruction("ldr x0, [sp, #0]");                            // restore the bool result
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 32");                                 // scratch: [rsp+0] path ptr, [rsp+8] path len, [rsp+16] mode
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the path pointer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the path length
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // save the mode value
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // rax = 1 when the scheme matches a registered wrapper
            emitter.instruction("test rax, rax");                               // matched a registered wrapper scheme?
            emitter.instruction(&format!("jnz {}", wrapper));                   // registered wrapper scheme → wrapper stream_metadata
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // libc path ptr → rax
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // libc path len → rdx
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // libc mode → rdi (secondary integer arg)
            emitter.instruction("add rsp, 32");                                 // release the scratch frame before the call (libc helper runs at the original rsp)
            abi::emit_call_label(emitter, "__rt_chmod");                        // normal path: libc chmod(path, mode)
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the mode integer
            emit_box_int_as_mixed(emitter);                                     // box $value as mixed → rax = owned Mixed(int)
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // stash the boxed value pointer (mode slot reused)
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // wrapper path ptr → rdi
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // wrapper path len → rsi
            emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // stream_metadata vtable slot
            emitter.instruction(&format!("mov rcx, {}", STREAM_META_ACCESS));   // option = STREAM_META_ACCESS
            emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                // value = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's stream_metadata method
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // stash the bool result across the value release
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the boxed value pointer
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the boxed $value (caller owns; the method borrowed it)
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore the bool result
            emitter.instruction("add rsp, 32");                                 // release the scratch frame
            emitter.label(&after);
        }
    }
    Some(PhpType::Bool)
}
