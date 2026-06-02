//! Purpose:
//! Emits PHP `touch` filesystem mutation builtin calls.
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
use crate::codegen::functions::infer_contextual_type;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const TOUCH_ATIME_NOW: u8 = 1;
const TOUCH_MTIME_NOW: u8 = 2;
const TOUCH_BOTH_NOW: u8 = TOUCH_ATIME_NOW | TOUCH_MTIME_NOW;

/// `stream_metadata` vtable slot index in the per-class user-wrapper vtable.
const STREAM_METADATA_SLOT: usize = 14;
/// PHP `STREAM_META_TOUCH` option value (`touch`-style metadata change).
const STREAM_META_TOUCH: usize = 1;

/// Emits code for the PHP `touch()` builtin.
///
/// # Arguments
/// - `name`: Unused name matching the dispatcher signature.
/// - `args`: Expression tree for path, optional mtime, and optional atime.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and function metadata.
/// - `data`: Data section for string literals and relocations.
///
/// # Returns
/// Always returns `Some(PhpType::Bool)` — `touch()` returns a boolean in PHP.
///
/// # Behavior
/// Emits path pointer/length in x1/x2 (ARM64) or rdi/rsi (x86_64), then
/// timestamp fields in x3/x4/x5 (ARM64) or rdi/rsi/rcx (x86_64), and calls
/// `__rt_touch`. Timestamp fields encode whether each time is "now" via the
/// `TOUCH_*_NOW` flags in the control byte.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("touch()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_touch_args_aarch64(args, emitter, ctx, data);
            emit_touch_tail_aarch64(emitter, ctx);
        }
        Arch::X86_64 => {
            emit_touch_args_x86_64(args, emitter, ctx, data);
            emit_touch_tail_x86_64(emitter, ctx);
        }
    }
    Some(PhpType::Bool)
}

/// Emits the wrapper-vs-libc dispatch tail for `touch()` on AArch64.
///
/// On entry the path occupies `x1`/`x2`, the mtime `x3`, the atime `x4`, and the
/// current-time flags `x5` (as left by `emit_touch_args_aarch64`). A registered
/// `scheme://` path builds the `[mtime, atime]` value array via
/// `__rt_touch_meta_array` and dispatches to the wrapper's
/// `stream_metadata($path, STREAM_META_TOUCH, $value)` (vtable slot 14),
/// releasing the boxed value afterwards; any other path calls libc `__rt_touch`.
/// The bool result is left in `x0`.
fn emit_touch_tail_aarch64(emitter: &mut Emitter, ctx: &mut Context) {
    let wrapper = ctx.next_label("touch_wrapper");
    let after = ctx.next_label("touch_after");
    emitter.instruction("sub sp, sp, #48");                                     // scratch: path ptr/len, mtime, atime, flags, result
    emitter.instruction("str x1, [sp, #0]");                                    // save the path pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the path length
    emitter.instruction("str x3, [sp, #16]");                                   // save the mtime seconds
    emitter.instruction("str x4, [sp, #24]");                                   // save the atime seconds
    emitter.instruction("str x5, [sp, #32]");                                   // save the current-time flags
    emitter.instruction("mov x0, x1");                                          // path_is_wrapper arg0 = path ptr
    emitter.instruction("mov x1, x2");                                          // path_is_wrapper arg1 = path len
    abi::emit_call_label(emitter, "__rt_path_is_wrapper");                      // x0 = 1 when the scheme matches a registered wrapper
    emitter.instruction(&format!("cbnz x0, {}", wrapper));                      // registered wrapper scheme → stream_metadata
    emitter.instruction("ldr x1, [sp, #0]");                                    // libc path ptr → x1
    emitter.instruction("ldr x2, [sp, #8]");                                    // libc path len → x2
    emitter.instruction("ldr x3, [sp, #16]");                                   // libc mtime → x3
    emitter.instruction("ldr x4, [sp, #24]");                                   // libc atime → x4
    emitter.instruction("ldr x5, [sp, #32]");                                   // libc current-time flags → x5
    emitter.instruction("add sp, sp, #48");                                     // release the scratch frame before the libc call
    abi::emit_call_label(emitter, "__rt_touch");                                // normal path: libc touch(path, mtime, atime, flags)
    emitter.instruction(&format!("b {}", after));                               // skip the wrapper path
    emitter.label(&wrapper);
    emitter.instruction("ldr x0, [sp, #16]");                                   // mtime → touch_meta_array arg0
    emitter.instruction("ldr x1, [sp, #24]");                                   // atime → touch_meta_array arg1
    emitter.instruction("ldr x2, [sp, #32]");                                   // flags → touch_meta_array arg2
    abi::emit_call_label(emitter, "__rt_touch_meta_array");                     // x0 = boxed Mixed([mtime, atime])
    emitter.instruction("str x0, [sp, #16]");                                   // stash the boxed value pointer (mtime slot reused)
    emitter.instruction("ldr x0, [sp, #0]");                                    // wrapper path ptr → x0
    emitter.instruction("ldr x1, [sp, #8]");                                    // wrapper path len → x1
    emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT));         // stream_metadata vtable slot
    emitter.instruction(&format!("mov x3, #{}", STREAM_META_TOUCH));            // option = STREAM_META_TOUCH
    emitter.instruction("ldr x4, [sp, #16]");                                   // value = boxed mixed pointer
    abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");                 // dispatch into the wrapper's stream_metadata
    emitter.instruction("str x0, [sp, #0]");                                    // stash the bool result across the value release
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the boxed value pointer
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the boxed $value (caller owns; the method borrowed it)
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the bool result
    emitter.instruction("add sp, sp, #48");                                     // release the scratch frame
    emitter.label(&after);
}

/// Emits the wrapper-vs-libc dispatch tail for `touch()` on x86_64.
///
/// On entry the path occupies `rax`/`rdx`, the mtime `rdi`, the atime `rsi`, and
/// the current-time flags `rcx` (as left by `emit_touch_args_x86_64`). Mirrors
/// `emit_touch_tail_aarch64`: a registered wrapper builds the value array and
/// dispatches to `stream_metadata`; any other path calls libc `__rt_touch`. The
/// bool result is left in `rax`.
fn emit_touch_tail_x86_64(emitter: &mut Emitter, ctx: &mut Context) {
    let wrapper = ctx.next_label("touch_wrapper");
    let after = ctx.next_label("touch_after");
    emitter.instruction("sub rsp, 48");                                         // scratch: path ptr/len, mtime, atime, flags, result
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // save the path pointer
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // save the path length
    emitter.instruction("mov QWORD PTR [rsp + 16], rdi");                       // save the mtime seconds
    emitter.instruction("mov QWORD PTR [rsp + 24], rsi");                       // save the atime seconds
    emitter.instruction("mov QWORD PTR [rsp + 32], rcx");                       // save the current-time flags
    emitter.instruction("mov rdi, rax");                                        // path_is_wrapper arg0 = path ptr
    emitter.instruction("mov rsi, rdx");                                        // path_is_wrapper arg1 = path len
    abi::emit_call_label(emitter, "__rt_path_is_wrapper");                      // rax = 1 when the scheme matches a registered wrapper
    emitter.instruction("test rax, rax");                                       // matched a registered wrapper scheme?
    emitter.instruction(&format!("jnz {}", wrapper));                           // registered wrapper scheme → stream_metadata
    emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                        // libc path ptr → rax
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // libc path len → rdx
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // libc mtime → rdi
    emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                       // libc atime → rsi
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // libc current-time flags → rcx
    emitter.instruction("add rsp, 48");                                         // release the scratch frame before the libc call
    abi::emit_call_label(emitter, "__rt_touch");                                // normal path: libc touch(path, mtime, atime, flags)
    emitter.instruction(&format!("jmp {}", after));                             // skip the wrapper path
    emitter.label(&wrapper);
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // mtime → touch_meta_array arg0
    emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                       // atime → touch_meta_array arg1
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // flags → touch_meta_array arg2
    abi::emit_call_label(emitter, "__rt_touch_meta_array");                     // rax = boxed Mixed([mtime, atime])
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // stash the boxed value pointer (mtime slot reused)
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // wrapper path ptr → rdi
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // wrapper path len → rsi
    emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT));         // stream_metadata vtable slot
    emitter.instruction(&format!("mov rcx, {}", STREAM_META_TOUCH));            // option = STREAM_META_TOUCH
    emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                        // value = boxed mixed pointer
    abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");                 // dispatch into the wrapper's stream_metadata
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // stash the bool result across the value release
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // reload the boxed value pointer
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the boxed $value (caller owns; the method borrowed it)
    emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                        // restore the bool result
    emitter.instruction("add rsp, 48");                                         // release the scratch frame
    emitter.label(&after);
}

/// Materializes timestamp arguments for the `touch()` call on ARM64.
///
/// # Arguments
/// - `args`: Expression tree for path, optional mtime, and optional atime.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context.
/// - `data`: Data section.
///
/// # Behavior
/// The path pointer/len are already in x1/x2 when this is called.
/// The control byte (x5) flags whether "now" is used for each timestamp:
/// - `TOUCH_BOTH_NOW`: both atime and mtime use current time; x3/x4 are ignored.
/// - Otherwise: x3 = mtime seconds, x4 = atime seconds (defaults to mtime when atime is NULL).
///
/// # Implementation notes
/// - `BothNow`: loads immediate zeros and `TOUCH_BOTH_NOW` into x3/x4/x5.
/// - `MtimeAlsoAtime`: evaluates `args[1]` into x0, copies to x3 and x4, sets control to 0.
/// - `ExplicitBoth`: nested evaluation of args[1] and args[2] with stack preservation
///   of path registers across the nested `emit_expr` calls.
fn emit_touch_args_aarch64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match touch_time_shape(args, ctx) {
        TouchTimeShape::BothNow => {
            emitter.instruction("mov x3, #0");                                  // ignored mtime seconds when runtime uses current time
            emitter.instruction("mov x4, #0");                                  // ignored atime seconds when runtime uses current time
            emitter.instruction(&format!("mov x5, #{}", TOUCH_BOTH_NOW));       // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path while mtime is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // mtime seconds
            emitter.instruction("mov x4, x0");                                  // atime defaults to mtime seconds
            emitter.instruction("mov x5, #0");                                  // both timestamp fields are explicit
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore path ptr/len
        }
        TouchTimeShape::ExplicitBoth => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path while timestamps are evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // save mtime seconds
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov x4, x0");                                  // atime seconds
            emitter.instruction("ldr x3, [sp], #16");                           // restore mtime seconds
            emitter.instruction("mov x5, #0");                                  // both timestamp fields are explicit
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore path ptr/len
        }
    }
}

/// Materializes timestamp arguments for the `touch()` call on x86_64.
///
/// # Arguments
/// - `args`: Expression tree for path, optional mtime, and optional atime.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context.
/// - `data`: Data section.
///
/// # Behavior
/// The path pointer/len are already in rdi/rsi when this is called.
/// The control byte (rcx) flags whether "now" is used for each timestamp:
/// - `TOUCH_BOTH_NOW`: both atime and mtime use current time; rdi/rsi are ignored.
/// - Otherwise: rdi = mtime seconds, rsi = atime seconds (defaults to mtime when atime is NULL).
///
/// # Implementation notes
/// - `BothNow`: loads immediate zeros and `TOUCH_BOTH_NOW` into rdi/rsi/rcx.
/// - `MtimeAlsoAtime`: uses `emit_push_reg_pair` to preserve rax/rdx across mtime evaluation,
///   then copies mtime into both rdi and rsi.
/// - `ExplicitBoth`: uses stack temporary to hold mtime across atime evaluation, with
///   aligned `sub rsp, 16` / `add rsp, 16` and `emit_push/pop_reg_pair` for path preservation.
fn emit_touch_args_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match touch_time_shape(args, ctx) {
        TouchTimeShape::BothNow => {
            emitter.instruction("mov rdi, 0");                                  // ignored mtime seconds when runtime uses current time
            emitter.instruction("mov rsi, 0");                                  // ignored atime seconds when runtime uses current time
            emitter.instruction(&format!("mov rcx, {}", TOUCH_BOTH_NOW));       // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path while mtime is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // mtime seconds
            emitter.instruction("mov rsi, rax");                                // atime defaults to mtime seconds
            emitter.instruction("mov rcx, 0");                                  // both timestamp fields are explicit
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore path ptr/len
        }
        TouchTimeShape::ExplicitBoth => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path while timestamps are evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("sub rsp, 16");                                 // reserve aligned temporary storage for mtime
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // save mtime seconds
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov rsi, rax");                                // atime seconds
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // restore mtime seconds
            emitter.instruction("add rsp, 16");                                 // release mtime temporary storage
            emitter.instruction("mov rcx, 0");                                  // both timestamp fields are explicit
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore path ptr/len
        }
    }
}

enum TouchTimeShape {
    BothNow,
    MtimeAlsoAtime,
    ExplicitBoth,
}

/// Categorizes the number of explicit timestamp arguments to `touch()`.
///
/// # Arguments
/// - `args`: All arguments to the `touch()` call (path, optional mtime, optional atime).
/// - `ctx`: Codegen context for type inference.
///
/// # Returns
/// - `BothNow`: Zero explicit timestamps, or both explicitly NULL.
/// - `MtimeAlsoAtime`: One timestamp argument (mtime), atime defaults to mtime.
/// - `ExplicitBoth`: Both mtime and atime are provided and non-NULL.
fn touch_time_shape(args: &[Expr], ctx: &Context) -> TouchTimeShape {
    match args.len() {
        1 => TouchTimeShape::BothNow,
        2 if is_static_null(&args[1], ctx) => TouchTimeShape::BothNow,
        2 => TouchTimeShape::MtimeAlsoAtime,
        _ if is_static_null(&args[1], ctx) && is_static_null(&args[2], ctx) => {
            TouchTimeShape::BothNow
        }
        _ if is_static_null(&args[2], ctx) => TouchTimeShape::MtimeAlsoAtime,
        _ => TouchTimeShape::ExplicitBoth,
    }
}

/// Checks whether an expression is statically known to be NULL or void.
///
/// # Arguments
/// - `expr`: The expression to check.
/// - `ctx`: Codegen context for contextual type inference.
///
/// # Returns
/// `true` if `expr` is a `Null` literal or inferred as `PhpType::Void`.
/// Used by `touch_time_shape` to treat NULL timestamps as "use current time".
fn is_static_null(expr: &Expr, ctx: &Context) -> bool {
    matches!(expr.kind, ExprKind::Null) || infer_contextual_type(expr, ctx) == PhpType::Void
}
