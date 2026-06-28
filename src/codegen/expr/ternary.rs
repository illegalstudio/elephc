//! Purpose:
//! Lowers PHP ternary and elvis expressions with branch labels and merged result storage.
//! Preserves short-circuit behavior while producing one expression result for callers.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Only the selected branch may run, and branch result types must be coerced into a common register shape.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, functions};
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

use super::{coerce_result_to_type, coerce_to_string, coerce_to_truthiness, emit_expr};

/// Emits a full ternary expression (`cond ? then : else`).
///
/// Evaluates `condition`, branches to `else_label` if zero, then emits `then_expr` and jumps to `end_label`.
/// Falls through to `else_label` for the else branch. Both branches are coerced to a common `result_ty`.
/// Returns the unified result type after both branches have been emitted.
pub(super) fn emit_ternary(
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let else_label = ctx.next_label("tern_else");
    let end_label = ctx.next_label("tern_end");
    emitter.comment("ternary");

    let cond_ty = emit_expr(condition, emitter, ctx, data);
    coerce_to_truthiness(emitter, ctx, &cond_ty);

    // -- branch based on ternary condition --
    abi::emit_branch_if_int_result_zero(emitter, &else_label);

    let result_ty = infer_branch_result_type(then_expr, else_expr, ctx);

    let then_ty = emit_expr(then_expr, emitter, ctx, data);
    coerce_branch_result(emitter, ctx, data, then_expr, &then_ty, &result_ty);
    abi::emit_jump(emitter, &end_label);                                        // skip else branch after evaluating then-expr

    emitter.label(&else_label);
    let else_ty = emit_expr(else_expr, emitter, ctx, data);
    coerce_branch_result(emitter, ctx, data, else_expr, &else_ty, &result_ty);

    emitter.label(&end_label);
    result_ty
}

/// Emits the short ternary / elvis operator (`value ?: default`).
///
/// Emits `value` and saves the result before testing its truthiness.
/// If truthy, restores the saved value and jumps to `end_label`. Otherwise falls through to emit `default`.
/// Both branches are coerced to a common `result_ty`. Returns the unified result type.
pub(super) fn emit_short_ternary(
    value: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let default_label = ctx.next_label("short_tern_default");
    let end_label = ctx.next_label("short_tern_end");
    emitter.comment("short ternary");

    let result_ty = infer_branch_result_type(value, default, ctx);
    let value_ty = emit_expr(value, emitter, ctx, data);
    abi::emit_push_result_value(emitter, &value_ty);
    coerce_to_truthiness(emitter, ctx, &value_ty);

    // -- branch based on the saved left value's truthiness --
    abi::emit_branch_if_int_result_zero(emitter, &default_label);

    pop_saved_result_value(emitter, &value_ty);
    coerce_branch_result(emitter, ctx, data, value, &value_ty, &result_ty);
    abi::emit_jump(emitter, &end_label);                                        // skip fallback after restoring truthy left value

    emitter.label(&default_label);
    discard_saved_result_value(emitter, &value_ty);
    let default_ty = emit_expr(default, emitter, ctx, data);
    coerce_branch_result(emitter, ctx, data, default, &default_ty, &result_ty);

    emitter.label(&end_label);
    result_ty
}

/// Infers the unified result type for the two ternary branches.
///
/// Uses a dummy signature to infer the type of each branch via `functions::infer_local_type_with_ctx`.
/// Returns the common type: exact match if equal, `Mixed` if one branch is `Void`,
/// `Str` if either is `Str`, `Float` if either is `Float`, otherwise the left type.
fn infer_branch_result_type(left: &Expr, right: &Expr, ctx: &Context) -> PhpType {
    let dummy_sig = FunctionSig {
        params: vec![],
        defaults: vec![],
        return_type: PhpType::Int,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![],
        declared_params: vec![],
        variadic: None,
        deprecation: None,
    };
    let left_ty = functions::infer_local_type_with_ctx(left, &dummy_sig, ctx);
    let right_ty = functions::infer_local_type_with_ctx(right, &dummy_sig, ctx);
    if left_ty == right_ty {
        left_ty
    } else if left_ty == PhpType::Void || right_ty == PhpType::Void {
        PhpType::Mixed
    } else if left_ty == PhpType::Str || right_ty == PhpType::Str {
        PhpType::Str
    } else if left_ty == PhpType::Float || right_ty == PhpType::Float {
        PhpType::Float
    } else {
        left_ty
    }
}

/// Coerces a branch result from `branch_ty` to `result_ty` in place.
///
/// No-op if types already match. Handles `Mixed`/`Union` boxing, string coercion,
/// int-to-float promotion, and general type coercion via `coerce_result_to_type`.
/// When the branch expression owns a `Mixed` that is being coerced to a non-Mixed type,
/// preserves the mixed value across coercion and releases it afterward to keep ownership balanced.
fn coerce_branch_result(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    branch_expr: &Expr,
    branch_ty: &PhpType,
    result_ty: &PhpType,
) {
    if result_ty == branch_ty {
        return;
    }
    if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, branch_ty);
    } else if *result_ty == PhpType::Str {
        coerce_to_string(emitter, ctx, data, branch_ty);
    } else if *result_ty == PhpType::Float && *branch_ty == PhpType::Int {
        abi::emit_int_result_to_float_result(emitter);                          // convert int to float for unified result type
    } else if crate::codegen::expr::can_coerce_result_to_type(branch_ty, result_ty) {
        let release_mixed_after_coerce = !matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
            && crate::codegen::stmt::helpers::should_release_owned_mixed_after_coerce(
                branch_expr,
                branch_ty,
                result_ty,
            );
        if release_mixed_after_coerce {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        }
        coerce_result_to_type(emitter, ctx, data, branch_ty, result_ty);
        if release_mixed_after_coerce {
            crate::codegen::stmt::helpers::release_preserved_mixed_after_coercion(
                emitter,
                result_ty,
            );
        }
    }
}

/// Pops the saved result value from the runtime stack into the appropriate result register(s).
///
/// Matches on `PhpType::codegen_repr()` to emit the correct pop instruction(s):
///   - Scalar/integer types → pop `int_result_reg`
///   - Float → pop `float_result_reg`
///   - String → pop register pair (ptr + len)
///   - Void/Never → nothing
fn pop_saved_result_value(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
        }
        PhpType::Void | PhpType::Never => {}
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(emitter);
            abi::emit_pop_reg_pair(emitter, abi::int_result_reg(emitter), tag_reg);
        }
    }
}

/// Discards the saved result value from the runtime stack without materializing it as a result.
///
/// Simply pops the value off the stack; used when the short-ternary condition is falsy
/// and the saved left value is not needed.
fn discard_saved_result_value(emitter: &mut Emitter, ty: &PhpType) {
    pop_saved_result_value(emitter, ty);
}
