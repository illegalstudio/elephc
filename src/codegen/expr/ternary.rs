use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, functions};
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

use super::{coerce_to_string, coerce_to_truthiness, emit_expr};

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
    coerce_branch_result(emitter, ctx, data, &then_ty, &result_ty);
    abi::emit_jump(emitter, &end_label);                                        // skip else branch after evaluating then-expr

    emitter.label(&else_label);
    let else_ty = emit_expr(else_expr, emitter, ctx, data);
    coerce_branch_result(emitter, ctx, data, &else_ty, &result_ty);

    emitter.label(&end_label);
    result_ty
}

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
    coerce_branch_result(emitter, ctx, data, &value_ty, &result_ty);
    abi::emit_jump(emitter, &end_label);                                        // skip fallback after restoring truthy left value

    emitter.label(&default_label);
    discard_saved_result_value(emitter, &value_ty);
    let default_ty = emit_expr(default, emitter, ctx, data);
    coerce_branch_result(emitter, ctx, data, &default_ty, &result_ty);

    emitter.label(&end_label);
    result_ty
}

fn infer_branch_result_type(left: &Expr, right: &Expr, ctx: &Context) -> PhpType {
    let dummy_sig = FunctionSig {
        params: vec![],
        defaults: vec![],
        return_type: PhpType::Int,
        ref_params: vec![],
        declared_params: vec![],
        variadic: None,
    };
    let left_ty = functions::infer_local_type_with_ctx(left, &dummy_sig, ctx);
    let right_ty = functions::infer_local_type_with_ctx(right, &dummy_sig, ctx);
    if left_ty == right_ty {
        left_ty
    } else if left_ty == PhpType::Str || right_ty == PhpType::Str {
        PhpType::Str
    } else if left_ty == PhpType::Float || right_ty == PhpType::Float {
        PhpType::Float
    } else {
        left_ty
    }
}

fn coerce_branch_result(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    branch_ty: &PhpType,
    result_ty: &PhpType,
) {
    if result_ty == branch_ty {
        return;
    }
    if *result_ty == PhpType::Str {
        coerce_to_string(emitter, ctx, data, branch_ty);
    } else if *result_ty == PhpType::Float && *branch_ty == PhpType::Int {
        abi::emit_int_result_to_float_result(emitter);                          // convert int to float for unified result type
    }
}

fn pop_saved_result_value(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
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
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
        }
        PhpType::Void => {}
    }
}

fn discard_saved_result_value(emitter: &mut Emitter, ty: &PhpType) {
    pop_saved_result_value(emitter, ty);
}
