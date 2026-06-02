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

use super::path_op_wrapper::{
    emit_owner_group_name_wrapper_dispatch, emit_owner_group_wrapper_dispatch, STREAM_META_GROUP,
    STREAM_META_GROUP_NAME,
};

/// Emits the `chgrp($path, $group)` builtin call.
///
/// `args[0]` is the path (string expression) and `args[1]` is the group principal,
/// which may be a string (group name) or integer (GID).
///
/// On a registered `scheme://` path the call dispatches to the wrapper's
/// `stream_metadata($path, $option, $value)` (vtable slot 14): an integer gid uses
/// `STREAM_META_GROUP` with the gid boxed as `mixed`, a string name uses
/// `STREAM_META_GROUP_NAME` with the name boxed as `mixed`. A non-wrapper path uses
/// libc `__rt_chown(path, -1, gid)` (integer) or `__rt_chgrp_group(path, name)`
/// (string), leaving the owner unchanged.
///
/// Returns `PhpType::Bool` (true = success, false = failure from runtime).
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
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path ptr/len while the group is evaluated
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emit_owner_group_name_wrapper_dispatch(
                    emitter,
                    ctx,
                    STREAM_META_GROUP_NAME,
                    "__rt_chgrp_group",
                ); // wrapper stream_metadata(GROUP_NAME) or libc chgrp_group
            } else {
                emit_owner_group_wrapper_dispatch(emitter, ctx, STREAM_META_GROUP); // wrapper stream_metadata(GROUP) or libc chown
            }
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path ptr/len while the group is evaluated
            let principal_ty = emit_expr(&args[1], emitter, ctx, data);
            if principal_ty == PhpType::Str {
                emit_owner_group_name_wrapper_dispatch(
                    emitter,
                    ctx,
                    STREAM_META_GROUP_NAME,
                    "__rt_chgrp_group",
                ); // wrapper stream_metadata(GROUP_NAME) or libc chgrp_group
            } else {
                emit_owner_group_wrapper_dispatch(emitter, ctx, STREAM_META_GROUP); // wrapper stream_metadata(GROUP) or libc chown
            }
        }
    }
    Some(PhpType::Bool)
}
