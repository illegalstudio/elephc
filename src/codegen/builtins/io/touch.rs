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
        Arch::AArch64 => emit_touch_args_aarch64(args, emitter, ctx, data),
        Arch::X86_64 => emit_touch_args_x86_64(args, emitter, ctx, data),
    }
    abi::emit_call_label(emitter, "__rt_touch");                                // call the target-aware runtime helper
    Some(PhpType::Bool)
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
