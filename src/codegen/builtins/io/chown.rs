//! Purpose:
//! Emits PHP `chown` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `chown($path, $owner)` builtin call.
///
/// `args[0]` is the path (string expression) and `args[1]` is the owner principal,
/// which may be a string (user name) or integer (UID). GID is unconditionally set to
/// `-1` to leave the group unchanged.
///
/// On AArch64: path pointer/length in `x1`/`x2`, owner in `x3`/`x4` (string) or `x3` (int).
/// On x86_64: path pointer/length in `rax`/`rdx`, owner in `rdi`/`rsi` (string) or `rdi` (int).
///
/// Calls `__rt_chown` for numeric UID or `__rt_chown_user` for string user name.
/// Returns `PhpType::Bool` (true = success, false = failure from runtime).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("chown()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path ptr/len while uid is evaluated
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov x3, x1");                              // user-name pointer → runtime string slot
                emitter.instruction("mov x4, x2");                              // user-name length → runtime string slot
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown_user");               // resolve user name and call libc chown()
            } else {
                emitter.instruction("mov x3, x0");                              // uid → runtime uid register
                emitter.instruction("mov x4, #-1");                             // gid = -1 (leave group unchanged)
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, uid, -1)
            }
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path ptr/len
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov rdi, rax");                            // user-name pointer → runtime string slot
                emitter.instruction("mov rsi, rdx");                            // user-name length → runtime string slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown_user");               // resolve user name and call libc chown()
            } else {
                emitter.instruction("mov rdi, rax");                            // uid → secondary integer arg slot
                emitter.instruction("mov rsi, -1");                             // gid = -1 (leave group unchanged)
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, uid, -1)
            }
        }
    }
    Some(PhpType::Bool)
}
