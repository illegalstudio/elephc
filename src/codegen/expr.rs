//! Purpose:
//! Dispatches expression AST nodes into focused lowering modules and shared coercion helpers.
//! Defines result conventions for scalars, strings, arrays, objects, calls, and special PHP operators.
//!
//! Called from:
//! - `crate::codegen::stmt`, `crate::codegen::functions`, and top-level emission
//!
//! Key details:
//! - Each expression leaves its result in the type-specific ABI result registers expected by callers.

pub(crate) mod arrays;
mod assignment;
mod binops;
mod chains;
/// calls
pub(crate) mod calls;
mod coerce;
mod compare;
mod diagnostics;
mod helpers;
/// objects
pub(crate) mod objects;
mod ownership;
mod scalars;
mod ternary;
mod variables;

use super::abi;
use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

pub(crate) use helpers::{can_coerce_result_to_type, coerce_result_to_type};
pub(crate) use objects::{emit_method_call_with_pushed_args, push_magic_property_name_arg};
pub(crate) use ownership::{
    expr_result_heap_ownership, string_result_is_owned_call_temp,
    string_result_uses_transient_concat_buffer,
};
pub use coerce::{
    coerce_null_to_zero, coerce_to_int, coerce_to_string, coerce_to_string_releasing_owned,
    coerce_to_truthiness,
};
use helpers::{retain_borrowed_heap_arg, widen_codegen_type};

/// Dispatches an expression AST node to the appropriate lowering module.
 ///
 /// Returns the resulting `PhpType` after code generation. Result values follow
 /// target ABI conventions: integers in `x0`, floats in `d0`, strings in `x1` (ptr)
 /// and `x2` (len). For expressions that emit no value (e.g., `Throw`), returns the
 /// bottom type for the context.
 ///
 /// Handles nullsafe chains first via `chains::emit_nullsafe_postfix_chain` before
 /// falling through to the standard dispatch table. All other `ExprKind` variants
 /// are delegated to their respective submodules.
pub fn emit_expr(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if let Some(ty) = chains::emit_nullsafe_postfix_chain(expr, emitter, ctx, data) {
        return ty;
    }

    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
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
        ExprKind::ArrayLiteral(elems) => arrays::emit_array_literal(elems, emitter, ctx, data),
        ExprKind::ArrayLiteralAssoc(pairs) if pairs.is_empty() => {
            arrays::emit_empty_assoc_array_literal(PhpType::Mixed, PhpType::Mixed, emitter)
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            arrays::emit_assoc_array_literal(pairs, emitter, ctx, data)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => arrays::emit_match_expr(subject, arms, default, emitter, ctx, data),
        ExprKind::ArrayAccess { array, index } => {
            arrays::emit_array_access(array, index, emitter, ctx, data)
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
        ExprKind::ErrorSuppress(inner) => {
            diagnostics::emit_error_suppress(inner, emitter, ctx, data)
        }
        ExprKind::Print(inner) => {
            emit_print_expr(inner, emitter, ctx, data)
        }
        ExprKind::NullCoalesce { value, default } => {
            compare::emit_null_coalesce(value, default, emitter, ctx, data)
        }
        ExprKind::Pipe { value, callable } => {
            calls::emit_pipe(value, callable, expr.span, emitter, ctx, data)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            assignment::emit_assignment_expr(
                target,
                value,
                result_target.as_deref(),
                prelude,
                conditional_value_temp.as_deref(),
                emitter,
                ctx,
                data,
            )
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
        } => ternary::emit_ternary(condition, then_expr, else_expr, emitter, ctx, data),
        ExprKind::ShortTernary { value, default } => {
            ternary::emit_short_ternary(value, default, emitter, ctx, data)
        }
        ExprKind::Cast { target, expr } => compare::emit_cast(target, expr, emitter, ctx, data),
        ExprKind::FunctionCall { name, args } => {
            if ctx.extern_functions.contains_key(name.as_str()) {
                return super::ffi::emit_extern_call(name.as_str(), args, expr.span, emitter, ctx, data);
            }
            if let Some(ty) =
                super::builtins::emit_builtin_call(name.as_str(), args, expr.span, emitter, ctx, data)
            {
                return ty;
            }
            calls::emit_function_call(name.as_str(), args, emitter, ctx, data)
        }
        ExprKind::Closure {
            params,
            return_type,
            body,
            is_arrow: _,
            is_static: _,
            variadic,
            variadic_type: _,
            captures,
            capture_refs,
            by_ref_return: _,
        } => calls::emit_closure(
            params,
            variadic,
            return_type,
            body,
            captures,
            capture_refs,
            emitter,
            ctx,
            data,
        ),
        ExprKind::FirstClassCallable(target) => {
            calls::emit_first_class_callable(target, emitter, ctx, data)
        }
        ExprKind::ClosureCall { var, args } => {
            calls::emit_closure_call(var, args, emitter, ctx, data)
        }
        ExprKind::ExprCall { callee, args } => {
            if let Some(ret_ty) =
                calls::emit_callable_array_literal_call(callee, args, emitter, ctx, data)
            {
                return ret_ty;
            }
            let loaded_callee_ty = emit_expr(callee, emitter, ctx, data);
            calls::emit_loaded_expr_call(callee, args, &loaded_callee_ty, emitter, ctx, data)
        }
        ExprKind::ConstRef(name) => {
            let (value, ty) = match ctx.constants.get(name.as_str()) {
                Some(c) => c.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined constant {}", name));
                    return PhpType::Int;
                }
            };
            let is_literal_constant = matches!(
                value,
                ExprKind::IntLiteral(_)
                    | ExprKind::FloatLiteral(_)
                    | ExprKind::StringLiteral(_)
                    | ExprKind::BoolLiteral(_)
                    | ExprKind::Null
            );
            let synthetic_expr = Expr::new(value, expr.span);
            let emitted_ty = emit_expr(&synthetic_expr, emitter, ctx, data);
            if is_literal_constant {
                ty
            } else {
                emitted_ty
            }
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
        ExprKind::InstanceOf { value, target } => {
            objects::emit_instanceof(value, target, emitter, ctx, data)
        }
        ExprKind::Spread(inner) => {
            // Spread is handled at call site / array literal level.
            // If we reach here, just evaluate the inner expression.
            emit_expr(inner, emitter, ctx, data)
        }
        ExprKind::NamedArg { value, .. } => emit_expr(value, emitter, ctx, data),
        ExprKind::NewObject { class_name, args } => {
            objects::emit_new_object(class_name.as_str(), args, emitter, ctx, data)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            objects::emit_new_dynamic(name_expr, args, emitter, ctx, data)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => objects::emit_new_dynamic_object(
            class_name,
            fallback_class.as_str(),
            required_parent.as_str(),
            args,
            emitter,
            ctx,
            data,
        ),
        ExprKind::PropertyAccess { object, property } => {
            objects::emit_property_access(object, property, emitter, ctx, data)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            objects::emit_dynamic_property_access(object, property, emitter, ctx, data)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            objects::emit_nullsafe_property_access(object, property, emitter, ctx, data)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            objects::emit_nullsafe_dynamic_property_access(object, property, emitter, ctx, data)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            objects::emit_static_property_access(receiver, property, emitter, ctx, data)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => objects::emit_method_call(object, method, args, emitter, ctx, data),
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => objects::emit_nullsafe_method_call(object, method, args, emitter, ctx, data),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => objects::emit_static_method_call(receiver, method, args, emitter, ctx, data),
        ExprKind::This => {
            variables::emit_this(emitter, ctx)
        }
        ExprKind::PtrCast { target_type, expr } => {
            emitter.comment(&format!("ptr_cast<{}>()", target_type));
            emit_expr(expr, emitter, ctx, data);
            // Value stays in x0 unchanged — only the type tag changes
            PhpType::Pointer(Some(target_type.clone()))
        }
        ExprKind::ClassConstant { receiver } => {
            objects::emit_class_constant(receiver, emitter, ctx, data)
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            objects::emit_scoped_constant_access(receiver, name, emitter, ctx, data)
        }
        ExprKind::NewScopedObject { receiver, args } => {
            objects::emit_new_scoped_object(receiver, args, emitter, ctx, data)
        }
        ExprKind::Yield { .. } | ExprKind::YieldFrom(_) => {
            unreachable!("yield expressions must be lowered by the generator-function codegen path")
        }
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before codegen")
        }
    }
}

/// Emits a PHP `print` expression: writes `inner` to stdout and returns integer `1`.
 ///
 /// PHP print always succeeds and evaluates to `1`. The result is placed in
 /// `int_result_reg` per ABI convention.
fn emit_print_expr(
    inner: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("print expression");
    super::stmt::emit_expr_to_stdout(inner, emitter, ctx, data);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    PhpType::Int
}

/// Delegates binary operation code generation to `binops::emit_binop`.
 ///
 /// Returns the `PhpType` produced by the operation, which depends on the operand types
 /// and the operator (e.g., int+int → int, int+str → str, etc.).
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

/// Saves the current concat offset before a nested function call.
/// On ARM64 this pushes the offset onto a temporary stack; on x86_64 it spills
/// the offset into a dedicated frame slot when one is allocated.
pub(crate) fn save_concat_offset_before_nested_call(emitter: &mut Emitter, ctx: &Context) {
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_concat_off", 0);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_push_reg(emitter, scratch);                                // save caller concat offset across nested call on the temporary stack
        }
        crate::codegen::platform::Arch::X86_64 => {
            if let Some(slot) = ctx.nested_concat_offset_offset {
                abi::store_at_offset(emitter, scratch, slot);                    // spill caller concat offset into the dedicated frame slot so nested x86_64 calls cannot clobber it
            } else {
                abi::emit_push_reg(emitter, scratch);                            // fall back to the temporary stack in raw emitter/unit-test contexts that do not allocate hidden frame slots
            }
        }
    }
}

/// Restores the concat offset after a nested function call returns.
/// If the return type is `Str`, persists the returned string before restoring the offset.
pub(crate) fn restore_concat_offset_after_nested_call(
    emitter: &mut Emitter,
    ctx: &Context,
    return_ty: &PhpType,
) {
    restore_concat_offset_after_nested_call_impl(emitter, ctx, *return_ty == PhpType::Str);
}

/// Restores the concat offset after a call that returns an owned string.
/// Does not persist the string (caller already handles ownership).
pub(crate) fn restore_concat_offset_after_owned_string_call(
    emitter: &mut Emitter,
    ctx: &Context,
) {
    restore_concat_offset_after_nested_call_impl(emitter, ctx, false);
}

/// Internal implementation for restoring concat offset after a nested call.
/// Optionally persists the returned string before restoring the offset.
fn restore_concat_offset_after_nested_call_impl(
    emitter: &mut Emitter,
    ctx: &Context,
    persist_string_result: bool,
) {
    if persist_string_result {
        abi::emit_call_label(emitter, "__rt_str_persist");                      // persist returned string before restoring caller concat cursor
    }
    let scratch = abi::temp_int_reg(emitter.target);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_pop_reg(emitter, scratch);                                // pop the saved caller concat offset from the temporary stack
        }
        crate::codegen::platform::Arch::X86_64 => {
            if let Some(slot) = ctx.nested_concat_offset_offset {
                abi::load_at_offset(emitter, scratch, slot);                    // reload the saved caller concat offset from the dedicated x86_64 frame slot
            } else {
                abi::emit_pop_reg(emitter, scratch);                            // fall back to the temporary stack in raw emitter/unit-test contexts that do not allocate hidden frame slots
            }
        }
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, "_concat_off", 0);
}
