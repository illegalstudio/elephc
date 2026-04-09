mod arrays;
mod binops;
pub(crate) mod calls;
mod coerce;
mod compare;
mod helpers;
mod objects;
mod ownership;
mod scalars;
mod variables;

use super::abi;
use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, TypeExpr};
use crate::types::FunctionSig;
use crate::types::PhpType;

/// Emits code to evaluate an expression.
/// Returns the type of the result.
/// - Strings: x1 = pointer, x2 = length
/// - Integers: x0 = value
pub fn emit_expr(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match &expr.kind {
        ExprKind::BoolLiteral(b) => {
            scalars::emit_bool_literal(*b, emitter)
        }
        ExprKind::Null => {
            scalars::emit_null_literal(emitter)
        }
        ExprKind::StringLiteral(s) => {
            scalars::emit_string_literal(s, emitter, data)
        }
        ExprKind::IntLiteral(n) => {
            scalars::emit_int_literal(*n, emitter)
        }
        ExprKind::FloatLiteral(f) => {
            scalars::emit_float_literal(*f, emitter, data)
        }
        ExprKind::Variable(name) => {
            variables::emit_variable(name, emitter, ctx)
        }
        ExprKind::Negate(inner) => {
            scalars::emit_negate(inner, emitter, ctx, data)
        }
        ExprKind::ArrayLiteral(elems) => emit_array_literal(elems, emitter, ctx, data),
        ExprKind::ArrayLiteralAssoc(pairs) => emit_assoc_array_literal(pairs, emitter, ctx, data),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => emit_match_expr(subject, arms, default, emitter, ctx, data),
        ExprKind::ArrayAccess { array, index } => {
            emit_array_access(array, index, emitter, ctx, data)
        }
        ExprKind::BufferNew { element_type, len } => {
            arrays::emit_buffer_new(element_type, len, emitter, ctx, data)
        }
        ExprKind::Not(inner) => {
            scalars::emit_not(inner, emitter, ctx, data)
        }
        ExprKind::BitNot(inner) => {
            scalars::emit_bit_not(inner, emitter, ctx, data)
        }
        ExprKind::Throw(inner) => {
            variables::emit_throw(inner, emitter, ctx, data)
        }
        ExprKind::NullCoalesce { value, default } => {
            emit_null_coalesce(value, default, emitter, ctx, data)
        }
        ExprKind::PreIncrement(name) => {
            variables::emit_pre_increment(name, emitter, ctx)
        }
        ExprKind::PostIncrement(name) => {
            variables::emit_post_increment(name, emitter, ctx)
        }
        ExprKind::PreDecrement(name) => {
            variables::emit_pre_decrement(name, emitter, ctx)
        }
        ExprKind::PostDecrement(name) => {
            variables::emit_post_decrement(name, emitter, ctx)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let else_label = ctx.next_label("tern_else");
            let end_label = ctx.next_label("tern_end");
            emitter.comment("ternary");
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &cond_ty);
            // -- branch based on ternary condition --
            abi::emit_branch_if_int_result_zero(emitter, &else_label);
                                                                  // -- determine result type: widen to the broader type --
            let dummy_sig = FunctionSig {
                params: vec![],
                defaults: vec![],
                return_type: PhpType::Int,
                ref_params: vec![],
                declared_params: vec![],
                variadic: None,
            };
            let then_syn = super::functions::infer_local_type_with_ctx(then_expr, &dummy_sig, ctx);
            let else_syn = super::functions::infer_local_type_with_ctx(else_expr, &dummy_sig, ctx);
            let result_ty = if then_syn == else_syn {
                then_syn
            } else if then_syn == PhpType::Str || else_syn == PhpType::Str {
                PhpType::Str
            } else if then_syn == PhpType::Float || else_syn == PhpType::Float {
                PhpType::Float
            } else {
                then_syn
            };
            let then_ty = emit_expr(then_expr, emitter, ctx, data);
            // -- coerce then-branch to result type if needed --
            if result_ty != then_ty {
                if result_ty == PhpType::Str {
                    coerce_to_string(emitter, ctx, data, &then_ty);
                } else if result_ty == PhpType::Float && then_ty == PhpType::Int {
                    emitter.instruction("scvtf d0, x0");                        // convert int to float for unified result type
                }
            }
            emitter.instruction(&format!("b {}", end_label));                   // skip else branch after evaluating then-expr
            emitter.label(&else_label);
            let else_ty = emit_expr(else_expr, emitter, ctx, data);
            // -- coerce else-branch to result type if needed --
            if result_ty != else_ty {
                if result_ty == PhpType::Str {
                    coerce_to_string(emitter, ctx, data, &else_ty);
                } else if result_ty == PhpType::Float && else_ty == PhpType::Int {
                    emitter.instruction("scvtf d0, x0");                        // convert int to float for unified result type
                }
            }
            emitter.label(&end_label);
            result_ty
        }
        ExprKind::Cast { target, expr } => emit_cast(target, expr, emitter, ctx, data),
        ExprKind::FunctionCall { name, args } => {
            if ctx.extern_functions.contains_key(name.as_str()) {
                return super::ffi::emit_extern_call(name.as_str(), args, emitter, ctx, data);
            }
            if let Some(ty) =
                super::builtins::emit_builtin_call(name.as_str(), args, emitter, ctx, data)
            {
                return ty;
            }
            emit_function_call(name.as_str(), args, emitter, ctx, data)
        }
        ExprKind::Closure {
            params,
            body,
            is_arrow: _,
            variadic,
            captures,
        } => emit_closure(params, variadic, body, captures, emitter, ctx, data),
        ExprKind::FirstClassCallable(target) => {
            emit_first_class_callable(target, emitter, ctx, data)
        }
        ExprKind::ClosureCall { var, args } => emit_closure_call(var, args, emitter, ctx, data),
        ExprKind::ExprCall { callee, args } => emit_expr_call(callee, args, emitter, ctx, data),
        ExprKind::ConstRef(name) => {
            let (value, _ty) = match ctx.constants.get(name.as_str()) {
                Some(c) => c.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined constant {}", name));
                    return PhpType::Int;
                }
            };
            let synthetic_expr = Expr::new(value, expr.span);
            emit_expr(&synthetic_expr, emitter, ctx, data)
        }
        ExprKind::EnumCase { enum_name, case_name } => {
            objects::emit_enum_case(enum_name.as_str(), case_name, emitter, ctx)
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
        ExprKind::Spread(inner) => {
            // Spread is handled at call site / array literal level.
            // If we reach here, just evaluate the inner expression.
            emit_expr(inner, emitter, ctx, data)
        }
        ExprKind::NamedArg { value, .. } => emit_expr(value, emitter, ctx, data),
        ExprKind::NewObject { class_name, args } => {
            emit_new_object(class_name.as_str(), args, emitter, ctx, data)
        }
        ExprKind::PropertyAccess { object, property } => {
            emit_property_access(object, property, emitter, ctx, data)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => emit_method_call(object, method, args, emitter, ctx, data),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => emit_static_method_call(receiver, method, args, emitter, ctx, data),
        ExprKind::This => {
            variables::emit_this(emitter, ctx)
        }
        ExprKind::PtrCast { target_type, expr } => {
            emitter.comment(&format!("ptr_cast<{}>()", target_type));
            emit_expr(expr, emitter, ctx, data);
            // Value stays in x0 unchanged — only the type tag changes
            PhpType::Pointer(Some(target_type.clone()))
        }
    }
}

fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_new_object(class_name, args, emitter, ctx, data)
}

fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_property_access(object, property, emitter, ctx, data)
}

fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_method_call(object, method, args, emitter, ctx, data)
}

pub(crate) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    objects::emit_method_call_with_pushed_args(class_name, method, arg_types, emitter, ctx)
}

pub(crate) fn push_magic_property_name_arg(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    objects::push_magic_property_name_arg(property, emitter, data)
}

fn emit_static_method_call(
    receiver: &crate::parser::ast::StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_static_method_call(receiver, method, args, emitter, ctx, data)
}

fn emit_array_literal(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_array_literal(elems, emitter, ctx, data)
}

fn emit_assoc_array_literal(
    pairs: &[(Expr, Expr)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_assoc_array_literal(pairs, emitter, ctx, data)
}

fn emit_match_expr(
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: &Option<Box<Expr>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_match_expr(subject, arms, default, emitter, ctx, data)
}

fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_array_access(array, index, emitter, ctx, data)
}

/// Coerce a value to string (x1=ptr, x2=len) for concatenation.
/// PHP behavior: false → "", true → "1", null → "", int → itoa
pub fn coerce_to_string(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
) {
    coerce::coerce_to_string(emitter, ctx, data, ty)
}

/// Replace null sentinel with 0 in x0 (for arithmetic/comparison with null).
/// Handles both compile-time null (Void type) and runtime null (variable
/// that was assigned null — sentinel value in x0).
pub fn coerce_null_to_zero(emitter: &mut Emitter, ty: &PhpType) {
    coerce::coerce_null_to_zero(emitter, ty)
}

/// Coerce any type to a truthiness value in x0 for use in conditions
/// (if, while, for, ternary, &&, ||). For strings, PHP treats both ""
/// and "0" as falsy. For other types, x0 already holds the truthiness.
pub fn coerce_to_truthiness(emitter: &mut Emitter, ctx: &mut Context, ty: &PhpType) {
    coerce::coerce_to_truthiness(emitter, ctx, ty)
}

/// Coerce any type to integer in x0 for loose comparison (==, !=).
fn emit_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    binops::emit_binop(left, op, right, emitter, ctx, data)
}

fn emit_function_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_function_call(name, args, emitter, ctx, data)
}

pub(crate) fn save_concat_offset_before_nested_call(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "x10", "_concat_off", 0);
    emitter.instruction("str x10, [sp, #-16]!");                                // save caller concat offset across nested call
}

pub(crate) fn restore_concat_offset_after_nested_call(emitter: &mut Emitter, return_ty: &PhpType) {
    if *return_ty == PhpType::Str {
        emitter.instruction("bl __rt_str_persist");                             // persist returned string before restoring caller concat cursor
    }
    emitter.instruction("ldr x10, [sp], #16");                                  // pop saved caller concat offset from stack
    abi::emit_store_reg_to_symbol(emitter, "x10", "_concat_off", 0);
}

pub(crate) fn expr_result_heap_ownership(expr: &Expr) -> HeapOwnership {
    ownership::expr_result_heap_ownership(expr)
}

fn retain_borrowed_heap_arg(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    helpers::retain_borrowed_heap_arg(emitter, expr, ty)
}

fn widen_codegen_type(a: &PhpType, b: &PhpType) -> PhpType {
    helpers::widen_codegen_type(a, b)
}

pub(crate) fn coerce_result_to_type(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: &PhpType,
) {
    helpers::coerce_result_to_type(emitter, ctx, data, source_ty, target_ty)
}

fn emit_closure(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_closure(params, variadic, body, captures, emitter, ctx, data)
}

fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_closure_call(var, args, emitter, ctx, data)
}

fn emit_first_class_callable(
    target: &CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_first_class_callable(target, emitter, ctx, data)
}

fn emit_expr_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_expr_call(callee, args, emitter, ctx, data)
}

fn emit_cast(
    target: &crate::parser::ast::CastType,
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    compare::emit_cast(target, expr, emitter, ctx, data)
}

fn emit_strict_compare(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    compare::emit_strict_compare(left, op, right, emitter, ctx, data)
}

fn emit_null_coalesce(
    value: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    compare::emit_null_coalesce(value, default, emitter, ctx, data)
}
