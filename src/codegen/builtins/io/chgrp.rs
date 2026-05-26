//! Purpose:
//! Emits PHP `chgrp` filesystem mutation builtin calls.
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

/// Emits the `chgrp` builtin call for both string group names and integer GIDs.
///
/// # Arguments
/// - `args[0]`: path (string) — pointer in x1/rax, length in x2/rdx
/// - `args[1]`: group — string → `__rt_chgrp_group` (resolves name via libc); int → `__rt_chown` with uid=-1
///
/// # ABI details
/// The path pointer/length (x1/x2 or rax/rdx) are preserved on the stack while the group
/// argument is evaluated, then restored before the runtime call. Integer GIDs pass through
/// the tertiary register (x4/rsi) with uid set to -1 to affect only the group ownership.
///
/// # Returns
/// `PhpType::Bool` — true on success, false on failure (matching PHP semantics).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("chgrp()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path ptr/len while gid is evaluated
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov x3, x1");                              // group-name pointer → runtime string slot
                emitter.instruction("mov x4, x2");                              // group-name length → runtime string slot
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chgrp_group");              // resolve group name and call libc chown()
            } else {
                emitter.instruction("mov x4, x0");                              // gid → runtime gid register
                emitter.instruction("mov x3, #-1");                             // uid = -1 (leave owner unchanged)
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, -1, gid)
            }
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path ptr/len
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emitter.instruction("mov rdi, rax");                            // group-name pointer → runtime string slot
                emitter.instruction("mov rsi, rdx");                            // group-name length → runtime string slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chgrp_group");              // resolve group name and call libc chown()
            } else {
                emitter.instruction("mov rsi, rax");                            // gid → tertiary integer arg slot
                emitter.instruction("mov rdi, -1");                             // uid = -1 (leave owner unchanged)
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore path ptr/len
                abi::emit_call_label(emitter, "__rt_chown");                    // call libc chown(path, -1, gid)
            }
        }
    }
    Some(PhpType::Bool)
}
