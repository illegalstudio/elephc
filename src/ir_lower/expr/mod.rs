//! Purpose:
//! Dispatches AST expression nodes into EIR values while preserving source-order
//! evaluation.
//!
//! Called from:
//! - `crate::ir_lower::stmt` and nested expression lowering.
//!
//! Key details:
//! - Simple scalar operations lower to concrete EIR arithmetic/string opcodes.
//! - Complex PHP runtime behavior lowers to high-level EIR opcodes with
//!   conservative effects until Phase 04 gives them target-specific meaning.

use crate::ir::{
    BlockId, CmpPredicate, Effects, Immediate, IrHeapKind, IrType, LocalKind, LocalSlotId,
    MixedNumericOp, Op, Ownership, PropertyFetchMode, Terminator, ValueId,
};
use crate::ir_lower::context::{
    value_ir_type, ClosureCapture, LoweredValue, LoweringContext, StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::ir_lower::function;
use crate::names::{php_symbol_key, property_hook_get_method, Name};
use crate::parser::ast::{
    is_compound_assignment_self_read, BinOp, CallableTarget, CastType, Expr, ExprKind,
    InstanceOfTarget, MagicConstant, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::span::Span;
use crate::types::checker::builtins::canonical_builtin_function_name;
use crate::types::{
    checker::infer_expr_type_syntactic, merge_array_key_types, normalized_array_key_type,
    ExternFunctionSig, FunctionSig, PhpType, ReturnArgAlias, ThrowAccessKind,
};
use std::collections::HashSet;

mod constants;
mod array_merge_values;
use array_merge_values::lower_array_value_for_merge;
mod filter;
mod late_bound_call;
mod nullsafe_chain;
mod ref_place_args;
mod boxed_user_sort;
mod scalar_literals;
mod numeric_binary;
mod integer_destinations;
pub(crate) use integer_destinations::coerce_integer_arithmetic;
use integer_destinations::lower_expr_for_parameter;
pub(crate) mod object_operators;
mod string_concat;
mod comparisons;
mod unary_logic;
mod branch_conditions;
mod lazy_branches;
mod pipe;
mod assignments;
mod string_offset_assign;
mod assignment_rhs_capture;
mod weak_map_places;
mod static_property_updates;
use static_property_updates::{finish_static_compound_receiver, guard_static_compound_property_read};
mod compat_preludes;
mod gradual_array_calls;
mod function_calls;
mod eval_barriers;
mod lazy_isset;
mod native_isset;
mod callable_probes;
mod descriptor_invoke;
pub(crate) mod call_operand_owners;
mod pending_owners;
mod descriptor_args;
mod descriptor_unpack;
mod static_array_callbacks;
mod callable_tracking;
mod callable_resolution;
mod unset;
mod unset_collection;
mod array_builtin_args;
mod array_push_spread;
mod borrowed_string_args;
mod weak_string_unions;
mod builtin_special_args;
mod call_arg_coercion;
mod positional_spreads;
mod dynamic_spreads;
mod dynamic_constructors;
use dynamic_constructors::*;
mod named_args;
mod named_spreads;
mod variadic_args;
mod call_return_types;
mod indexed_array_literals;
mod array_cast_source;
mod assoc_array_literals;
mod match_expr;
/// Scoped stored-cell leases for compiler-generated compound append updates.
mod append_updates;
pub(crate) use append_updates::store_alias as store_append_update_cell;
mod array_access;
mod string_offset_owners;
pub(in crate::ir_lower) mod array_access_types;
mod ternary_cast;
mod missing_string;
pub(crate) use missing_string::loop_region_missing_string_names;
mod closures;
mod closure_calls;
mod descriptor_calls;
mod object_construction;
pub(crate) mod property_access;
mod property_probes;
mod reference_returns;
use reference_returns::{
    begin_reference_return_call, finish_reference_return_call, finish_reference_return_value,
};
mod property_fetch_for_write;
mod method_calls;
mod closure_binding;
mod member_receivers;
mod enum_interface_calls;
mod enum_argument_guards;
mod enum_method_args;
mod enum_static_args;
mod enum_ref_args;
mod magic_call_fallback;
mod reflection_class_calls;
mod reflection_method_calls;
mod reflection_property_calls;
mod reflection_filters;
mod reflection_constructors;
mod reflection_static_properties;
mod class_introspection;
mod class_introspection_mixed;
mod core_introspection;
mod reflection_new_instance;
mod nullable_method_calls;
mod override_arity;
mod packed_builtin_args;
mod method_metadata;
mod static_method_calls;
mod definite_argument_errors;
mod scoped_values;
#[cfg(test)]
pub(crate) use scoped_values::DESCENDANT_QUERIES;
mod buffer_length;
mod generators;
mod instanceof_coercions;
mod merge_temps;
mod array_splice;

use array_splice::lower_array_splice_args;

use scalar_literals::*;
use filter::*;
use numeric_binary::*;
use string_concat::*;
use comparisons::*;
use unary_logic::*;
use lazy_branches::*;
use pipe::*;
use assignments::*;
use string_offset_assign::*;
use function_calls::*;
use eval_barriers::*;
use lazy_isset::*;
use native_isset::*;
use callable_probes::*;
use descriptor_invoke::*;
use call_operand_owners::*;
use descriptor_args::*;
use descriptor_unpack::*;
use static_array_callbacks::*;
use callable_tracking::*;
use callable_resolution::*;

/// Resolves the statically known receiver class for sibling statement lowering.
pub(in crate::ir_lower) fn instance_object_class_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> Option<String> {
    callable_resolution::instance_callable_object_class(ctx, object)
}
use unset::*;
use array_builtin_args::*;
use weak_string_unions::*;
use builtin_special_args::*;
pub(crate) use call_arg_coercion::apply_scalar_param_cast;
pub(crate) use assignments::lower_descendant_property_assign;
pub(crate) use callable_resolution::instance_callable_object_class_and_nullability;
pub(crate) use property_access::class_extends_class;
pub(in crate::ir_lower) use call_arg_coercion::param_accepts_object_without_string_coercion;
pub(in crate::ir_lower) use lazy_isset::emit_quiet_property_fetch;
use call_arg_coercion::*;
use positional_spreads::*;
use packed_builtin_args::*;
use named_args::*;
use named_spreads::*;

/// Lowers the quiet `isset($target)` probe used by write-context desugarings.
///
/// The target is parser-stabilized before this entry point is reached, so it is
/// safe for the ensuing write path to replay it after a negative probe.
pub(in crate::ir_lower) fn lower_synthesized_isset_for_write_target(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
) -> Option<LoweredValue> {
    let args = [target.clone()];
    lower_lazy_isset(ctx, "isset", &args, target)
}

/// Whether a property walk is known to end at a declared native array slot.
///
/// Write-context lowering may use a quiet `isset` probe only for this shape. A
/// virtual property can observe `__isset` separately from the ordinary `__get`
/// performed by an indirect write, so introducing that probe there would change
/// PHP-visible behavior.
pub(in crate::ir_lower) fn property_access_is_declared_native_array(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    if !matches!(
        // No member expression is at hand; the receiver's own span stands in for it.
        property_isset_action(ctx, object, property, object),
        Some(IssetPropertyAction::Initialized)
    ) {
        return false;
    }
    let Some((class_name, _)) = isset_object_expr_class(ctx, object) else {
        return false;
    };
    ctx.classes
        .get(class_name.trim_start_matches('\\'))
        .and_then(|class_info| class_info.visible_property(property))
        .is_some_and(|(_, (_, property_ty))| {
            matches!(
                property_ty.codegen_repr(),
                PhpType::Array(_) | PhpType::AssocArray { .. }
            )
        })
}
use variadic_args::*;
use indexed_array_literals::*;
use assoc_array_literals::*;
use match_expr::*;
use array_access::*;
use array_access_types::*;
use ternary_cast::*;
use closures::*;
use closure_calls::*;
use descriptor_calls::*;
use object_construction::*;
use property_access::*;
use property_probes::*;
use method_calls::*;
use closure_binding::*;
use member_receivers::{checked_member_receiver_type, lower_member_receiver, refine_member_receiver};
pub(crate) use member_receivers::lower_member_receiver_at_span;
use reflection_class_calls::*;
use reflection_method_calls::*;
use reflection_property_calls::*;
use reflection_filters::*;
use reflection_constructors::*;
use reflection_static_properties::*;
use class_introspection::*;
use core_introspection::*;
use reflection_new_instance::*;
use nullable_method_calls::*;
use override_arity::*;
pub(crate) use override_arity::reset_method_declarer_index;
#[cfg(test)]
pub(crate) use method_metadata::memoized_signature_stats;
#[cfg(test)]
pub(crate) use override_arity::WIDER_OVERRIDE_CANDIDATES;
use method_metadata::*;
pub(in crate::ir_lower) use method_metadata::method_signature;
use static_method_calls::*;
use scoped_values::*;
use generators::*;
use instanceof_coercions::*;
use merge_temps::*;

pub(in crate::ir_lower) use instanceof_coercions::{
    instanceof_branch_local_type, statically_known_instanceof_result,
};

pub(crate) use callable_resolution::{
    is_bound_closure_assignment_shape, lower_bound_closure_for_assignment,
};
pub(crate) use pending_owners::lower_with_pending_owners;
pub(crate) use branch_conditions::lower_condition;
pub(crate) use call_operand_owners::{
    pin_in_flight_owners, register_owned_call_operand, retire_owned_call_operand,
    root_owned_call_operand, unpin_in_flight_owners, unregister_owned_call_operand,
    PinnedInFlightOwner,
};
pub(crate) use property_access::singular_object_class;
pub(crate) use property_access::packed_receiver_class;
pub(crate) use property_access::extern_receiver_class;
pub(crate) use callable_tracking::{
    lower_callable_array_for_assignment, reflection_arg_array_binding_for_expr,
    reflection_class_binding_for_expr, reflection_function_binding_for_expr,
    reflection_method_binding_for_expr, reflection_property_binding_for_expr,
    static_callable_binding_for_expr,
};
#[allow(unused_imports)]
pub(crate) use callable_tracking::LoweredCallableArrayAssignment;
pub(crate) use closures::{body_contains_eval_call, body_writes_local, lower_closure_for_assignment};
pub(crate) use indexed_array_literals::{
    array_literal_type_for_ir, is_pointer_array_destination, lower_array_literal_with_expected_type,
    lower_expr_for_destination, lower_indexed_array_spread_into_array,
};
pub(crate) use array_access::{
    array_access_element_result_type, index_expr_key_type, lowered_index_expr_key_type,
    lower_array_access_from_lowered_receiver, lower_array_access_quietly,
    lower_by_ref_foreach_element_source,
};
pub(crate) use array_access_types::type_satisfies_array_access_for_ir;
pub(crate) use assignments::lower_null_coalesce_update_stmt;
pub(crate) use assignments::lower_null_coalesce_update_stmt_to;
pub(crate) use weak_map_places::lower_weak_map_incdec_stmt;
pub(crate) use weak_map_places::lower_weak_map_append;
pub(crate) use weak_map_places::lower_weak_map_denied_append;
use weak_map_places::lower_weak_map_incdec_expression;
pub(crate) use closures::expr_contains_eval_call;
pub(crate) use instanceof_coercions::{
    coerce_array_key_to_int_at_span, coerce_to_int_at_span, coerce_to_string_at_span,
};
pub(crate) use merge_temps::emit_bool_literal;
pub(in crate::ir_lower) use merge_temps::{store_value_into_temp, take_owned_temp};
pub(crate) use property_access::{
    lower_owned_ref_assign_array_elem, lower_ref_assign_array_elem, lower_ref_assign_call,
    lower_ref_assign_dynamic_property, lower_ref_assign_property, lower_ref_assign_static_property,
    prepare_scoped_addressable_ref_array_receiver,
    static_property_result_type,
};
pub(crate) use property_fetch_for_write::{
    is_static_property_write_chain, lower_static_property_write_chain,
    lower_static_property_write_chain_with_operands,
    by_ref_foreach_property_source_is_addressable, lower_by_ref_foreach_property_source,
    lower_nested_assignment_property_source,
};
pub(crate) use string_concat::string_value_needs_persistence;
pub(crate) use string_concat::persist_pending_string;
pub(crate) use string_concat::{
    cast_string_needs_owned_persist, persist_concat_lhs_if_rhs_can_reset, runtime_call_string_is_fresh,
    string_op_uses_scratch_storage,
};
pub(in crate::ir_lower) use assoc_array_literals::{
    array_access_expr_value_type_for_ir, lower_array_reference_or_value,
    method_call_expr_type_for_ir,
    property_access_expr_type_for_ir,
};
pub(super) use call_return_types::call_return_type;
pub(in crate::ir_lower) use merge_temps::coerce_container_to_mixed_payload;
pub(super) use call_return_types::eir_user_function_return_type;
pub(super) use method_metadata::closure_return_method_call_type;
pub(super) use nullsafe_chain::result_storage_type as nullsafe_chain_result_storage_type;
pub(super) use nullable_method_calls::lower_dynamic_method_call_with_receiver;
pub(super) use static_method_calls::static_method_call_expr_type_for_ir;
pub(crate) use static_method_calls::static_receiver_class_name;

/// Lowers an expression and returns its EIR value.
/// Calls a no-argument instance method on an already lowered receiver, for statement lowering
/// (`foreach` over an `IteratorAggregate` asks it for its iterator). The carrier expression only
/// supplies the span; the result type comes from the method's signature.
pub(crate) fn lower_zero_arg_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    span: Span,
) -> LoweredValue {
    let carrier = Expr::new(ExprKind::Null, span);
    nullable_method_calls::lower_method_call_with_receiver(
        ctx,
        object,
        method,
        &[],
        Op::MethodCall,
        &carrier,
    )
}

pub(crate) fn lower_expr(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
    ctx.expression_depth += 1;
    let value = lower_expr_inner(ctx, expr);
    ctx.expression_depth -= 1;
    if crate::ir_lower::context::is_by_ref_call_result_site(expr.span)
        && crate::types::checker::is_call_result_argument(expr)
    {
        // php binds the value to a temporary for the by-reference parameter and notices it.
        let message = ctx.intern_string("Notice: Only variables should be passed by reference\n");
        ctx.emit_void(
            Op::Warn,
            Vec::new(),
            Some(Immediate::Data(message)),
            Op::Warn.default_effects(),
            Some(expr.span),
        );
    }
    value
}

/// Dispatches one expression while its reference-call context remains distinct from nested operands.
fn lower_expr_inner(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
    if let Some(value) = definite_argument_errors::lower_definite_argument_mismatch(ctx, expr) {
        return value;
    }
    if let Some(value) = nullsafe_chain::lower(ctx, expr) {
        return value;
    }

    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::StringLiteral(value) => lower_string_literal(ctx, value, expr),
        ExprKind::IntLiteral(value) => lower_int_literal(ctx, *value, expr),
        ExprKind::FloatLiteral(value) => lower_float_literal(ctx, *value, expr),
        ExprKind::BoolLiteral(value) => lower_bool_literal(ctx, *value, expr),
        ExprKind::Null => lower_null(ctx, expr),
        ExprKind::Variable(name) => ctx.load_local(name, Some(expr.span)),
        ExprKind::BinaryOp { left, op, right } => lower_binary(ctx, left, op, right, expr),
        ExprKind::InstanceOf { value, target } => lower_instanceof(ctx, value, target, expr),
        ExprKind::Negate(inner) => lower_numeric_unary(ctx, inner, Op::INeg, Op::FNeg, expr),
        ExprKind::Not(inner) => lower_not(ctx, inner, expr),
        // php complements every byte of a string operand (brick/math's `~$bytes`).
        ExprKind::BitNot(inner)
            if merge_temps::materialized_expr_type_for_merge(ctx, inner) == PhpType::Str =>
        {
            lower_function_call(
                ctx,
                &crate::names::Name::unqualified(crate::string_bitnot_prelude::STRING_BITNOT_NAME),
                std::slice::from_ref(inner.as_ref()),
                expr,
            )
        }
        ExprKind::BitNot(inner) => lower_int_unary(ctx, inner, Op::IBitNot, expr),
        ExprKind::Throw(inner) => lower_throw_expr(ctx, inner, expr),
        ExprKind::ErrorSuppress(inner) => lower_error_suppress(ctx, inner, expr),
        ExprKind::Print(inner) => lower_print(ctx, inner, expr),
        ExprKind::NullCoalesce { value, default } => {
            lower_null_coalesce(ctx, value, default, expr)
        }
        ExprKind::Pipe { value, callable } => lower_pipe(ctx, value, callable, expr),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => lower_assignment_expr(
            ctx,
            target,
            value,
            result_target.as_deref(),
            prelude,
            conditional_value_temp.as_deref(),
            expr,
        ),
        ExprKind::PreIncrement(name) => lower_inc_dec(ctx, name, true, false, expr),
        ExprKind::PostIncrement(name) => lower_inc_dec(ctx, name, true, true, expr),
        ExprKind::PreDecrement(name) => lower_inc_dec(ctx, name, false, false, expr),
        ExprKind::PostDecrement(name) => lower_inc_dec(ctx, name, false, true, expr),
        ExprKind::FunctionCall { name, args } => lower_function_call(ctx, name, args, expr),
        ExprKind::ArrayLiteral(items) => lower_array_literal(ctx, items, expr),
        ExprKind::ArrayLiteralAssoc(pairs) => lower_assoc_array_literal(ctx, pairs, expr),
        ExprKind::ArrayReference(_) => {
            unreachable!("array-reference nodes are lowered by their containing literal")
        }
        ExprKind::ArrayLiteralMixed(entries) => lower_mixed_array_literal(ctx, entries, expr),
        ExprKind::Match { subject, arms, default } => lower_match(ctx, subject, arms, default.as_deref(), expr),
        ExprKind::ArrayAccess { array, index } => lower_array_access(ctx, array, index, expr),
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            lower_ternary(ctx, condition, then_expr, else_expr, expr)
        }
        ExprKind::ShortTernary { value, default } => {
            lower_short_ternary(ctx, value, default, expr)
        }
        ExprKind::Cast { target, expr: inner } => lower_cast(ctx, target, inner, expr),
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            return_type,
            body,
            captures,
            capture_refs,
            is_static,
            ..
        } => lower_closure(
            ctx,
            params,
            variadic.as_deref(),
            *variadic_by_ref,
            return_type.as_ref(),
            body,
            captures,
            capture_refs,
            expr,
            *is_static,
        ),
        ExprKind::NamedArg { value, .. } => lower_expr(ctx, value),
        ExprKind::Spread(inner) => lower_expr(ctx, inner),
        ExprKind::ClosureCall { var, args } => lower_closure_call(ctx, var, args, expr),
        ExprKind::ExprCall { callee, args } => lower_expr_call(ctx, callee, args, expr),
        ExprKind::ConstRef(name) => constants::lower_const_ref(ctx, name, expr),
        // `new Box<int>()` is rewritten to a `NewObject` naming the instantiated class long
        // before lowering; a template has no storage layout to construct.
        ExprKind::NewGeneric { .. } => unreachable!(
            "ExprKind::NewGeneric must be instantiated by generics::classes"
        ),
        ExprKind::NewObject { class_name, args } => lower_new_object(ctx, class_name, args, expr),
        ExprKind::Clone(inner) => lower_clone(ctx, inner, expr),
        ExprKind::NewDynamic { name_expr, args } => {
            lower_new_dynamic(ctx, name_expr, args, expr)
        }
        ExprKind::NewDynamicObject { class_name, fallback_class, required_parent, args } => {
            lower_new_dynamic_object(ctx, class_name, fallback_class, required_parent, args, expr)
        }
        ExprKind::PropertyAccess { object, property } => lower_property_get(ctx, object, property, Op::PropGet, expr),
        ExprKind::DynamicPropertyAccess { object, property } => lower_dynamic_property_get(ctx, object, property, expr),
        ExprKind::NullsafePropertyAccess { object, property } => {
            lower_property_get(ctx, object, property, Op::NullsafePropGet, expr)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            lower_dynamic_property_get(ctx, object, property, expr)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            lower_static_property_get(ctx, receiver, property, expr)
        }
        ExprKind::DynamicStaticPropertyAccess { receiver, property } => {
            lower_dynamic_static_property_get(ctx, receiver, property, expr)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => {
            if let Some(static_bind) = closure_bind_to_as_static_bind(ctx, object, method, args, expr) {
                return lower_expr(ctx, &static_bind);
            }
            lower_method_call(ctx, object, method, args, Op::MethodCall, expr)
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => lower_nullsafe_method_call(ctx, object, method, args, expr),
        ExprKind::NullsafeDynamicMethodCall { .. } => {
            unreachable!("nullsafe dynamic method calls are lowered as a nullsafe postfix chain")
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => lower_static_method_call(ctx, receiver, method, args, expr),
        ExprKind::FirstClassCallable(target) => lower_first_class_callable(ctx, target, expr),
        ExprKind::This => ctx.load_local("this", Some(expr.span)),
        ExprKind::PtrCast { target_type, expr: inner } => lower_ptr_cast(ctx, target_type, inner, expr),
        ExprKind::BufferNew { element_type, len } => lower_buffer_new(ctx, element_type, len, expr),
        ExprKind::ClassConstant { receiver } => lower_class_constant(ctx, receiver, expr),
        ExprKind::ObjectClassName { object } => lower_object_class_name(ctx, object, expr),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            lower_scoped_constant(ctx, receiver, name, expr)
        }
        ExprKind::DynamicScopedConstantAccess { receiver, name } => {
            lower_dynamic_scoped_constant(ctx, receiver, name, expr)
        }
        ExprKind::NewScopedObject { receiver, args } => lower_new_scoped_object(ctx, receiver, args, expr),
        ExprKind::MagicConstant(kind) => lower_magic_constant(ctx, kind, expr),
        ExprKind::Yield { key, value } => lower_yield(ctx, key.as_deref(), value.as_deref(), expr),
        ExprKind::YieldFrom(inner) => lower_yield_from(ctx, inner, expr),
    }
}

/// Rewrites `$closureValue->bindTo($newThis, $scope)` into `Closure::bind($closureValue, $newThis, $scope)`.
///
/// The two spellings are the same operation in PHP. Both now reach `closure_binding`, which
/// classifies the scope at run time; the static form additionally lowers a closure literal in the
/// bind's scope (`lower_static_closure_bind`). The rewrite is
/// limited to the case the instance form cannot handle: a receiver whose static type is a
/// callable VALUE (not a closure literal, whose body the instance form already specializes), with
/// an explicit scope and plain positional arguments. Everything else keeps its existing path.
/// Only a `mixed` receiver is rewritten (see the body).
fn closure_bind_to_as_static_bind(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<Expr> {
    if php_symbol_key(method) != "bindto"
        || args.len() < 2
        || matches!(object.kind, ExprKind::Closure { .. })
        || args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
    {
        return None;
    }
    // A `mixed` receiver holding a closure (`$m = H::$hs["k"] ??= static function () {...}`) binds
    // the same way, but only in a world where nothing else answers `bindTo()`: an object of a
    // class declaring its own `bindTo` must keep its method call.
    let receiver_repr = materialized_expr_type_for_merge(ctx, object).codegen_repr();
    let class_answers_bind_to = ctx
        .classes
        .iter()
        .any(|(name, class)| name.as_str() != "Closure" && class.methods.contains_key("bindto"));
    // Where a class does declare `bindTo` (symfony's `HttpOptions`), php decides on the object the
    // receiver holds, so the two calls are split at run time. The method call of the `else` arm
    // comes back here with the span already seen, and takes the ordinary path.
    if receiver_repr == PhpType::Mixed && class_answers_bind_to {
        if !is_repeatable_receiver(object) || !ctx.closure_bind_splits.insert(expr.span) {
            return None;
        }
        let closure_bind = closure_bind_call(object, args, expr);
        let is_closure = Expr::new(
            ExprKind::InstanceOf {
                value: Box::new(object.clone()),
                target: InstanceOfTarget::Name(Name::from("Closure".to_string())),
            },
            expr.span,
        );
        return Some(Expr::new(
            ExprKind::Ternary {
                condition: Box::new(is_closure),
                then_expr: Box::new(closure_bind),
                else_expr: Box::new(expr.clone()),
            },
            expr.span,
        ));
    }
    let mixed_closure_receiver = receiver_repr == PhpType::Mixed && !class_answers_bind_to;
    // A `callable` receiver keeps the instance form: `lower_closure_bind_method` classifies the
    // scope itself, and only the instance form retires a temporary receiver AFTER the receiver and
    // scope arguments, the order php destroys them in (`f()->bindTo(new A, new S)` runs `S`'s
    // destructor before the closure's captured `$this`'s). The static spelling retires the source
    // first, which is right for `Closure::bind()` and wrong here.
    if !mixed_closure_receiver {
        return None;
    }
    Some(closure_bind_call(object, args, expr))
}

/// Spells `$object->bindTo(...)` as `Closure::bind($object, ...)`.
fn closure_bind_call(object: &Expr, args: &[Expr], expr: &Expr) -> Expr {
    let mut bind_args = Vec::with_capacity(args.len() + 1);
    bind_args.push(object.clone());
    bind_args.extend(args.iter().cloned());
    Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(Name::from("Closure".to_string())),
            method: "bind".to_string(),
            args: bind_args,
        },
        expr.span,
    )
}

/// Whether reading `object` twice cannot run anything: a variable, or a property or element of one.
fn is_repeatable_receiver(object: &Expr) -> bool {
    match &object.kind {
        ExprKind::Variable(_) | ExprKind::This => true,
        ExprKind::PropertyAccess { object, .. } => is_repeatable_receiver(object),
        ExprKind::ArrayAccess { array, index } => {
            is_repeatable_receiver(array)
                && matches!(
                    index.kind,
                    ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::Variable(_)
                )
        }
        _ => false,
    }
}

/// Returns the effect set for one arithmetic opcode, dropping `MAY_THROW` when the right
/// operand is a literal that provably cannot raise PHP's arithmetic errors.
///
/// `Op::default_effects()` is opcode-level and must stay conservative: `/`, `%`, `<<`, and `>>`
/// can all raise a catchable error, so a value produced by them is not removable, hoistable, or
/// CSE-able. That would needlessly pessimize the common `$x << 3` / `$x / 2` shapes, where the
/// literal right operand rules the error out at compile time — the same test the AST effect
/// model (`optimize::effects::binary_op_may_throw`) applies. `MAY_FATAL` is left untouched so
/// the division opcodes keep exactly the impurity they had before the guards existed.
fn arithmetic_effects(op: Op, right: &Expr) -> Effects {
    let cannot_raise = match op {
        Op::IDiv | Op::ISDiv | Op::ISMod | Op::FDiv => matches!(
            &right.kind,
            ExprKind::IntLiteral(value) if *value != 0
        ) || matches!(
            &right.kind,
            ExprKind::FloatLiteral(value) if *value != 0.0
        ),
        Op::IShl | Op::IShrA => matches!(
            &right.kind,
            ExprKind::IntLiteral(value) if *value >= 0
        ),
        _ => false,
    };
    let effects = op.default_effects();
    if cannot_raise {
        effects.difference(Effects::MAY_THROW)
    } else {
        effects
    }
}

/// Lowers PHP's `++` / `--` on a value that may hold a string at runtime.
///
/// The operand is either a concrete `Str` load or a boxed `Mixed` load, and the result is
/// always a boxed `Mixed` cell: PHP's string increment can change the value's type
/// (`"9"++` is `int(10)`, `"az"++` is `"ba"`), so no concrete slot can hold both outcomes.
/// The `i64` immediate is the delta the runtime helper applies (`+1` or `-1`).
fn lower_str_inc_dec(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    increment: bool,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        Op::StrIncDec,
        vec![value.value],
        Some(Immediate::I64(if increment { 1 } else { -1 })),
        PhpType::Mixed,
        Op::StrIncDec.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `++`/`--` on a float local as PHP's `$f = $f ± 1.0`.
///
/// PHP never promotes or demotes a float here: the local stays a float and the operator
/// adds or subtracts exactly one. Post-forms return the value loaded before the store,
/// pre-forms re-read the local so the new value is the expression's result.
fn lower_float_inc_dec(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    increment: bool,
    post: bool,
    old: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let one = lower_float_literal(ctx, 1.0, expr);
    let op = if increment { Op::FAdd } else { Op::FSub };
    let new = ctx
        .builder
        .emit_with_effects(
            op,
            vec![old.value, one.value],
            None,
            IrType::F64,
            PhpType::Float,
            Ownership::NonHeap,
            op.default_effects(),
            Some(expr.span),
        )
        .expect("float inc/dec produces a value");
    let new = LoweredValue { value: new, ir_type: IrType::F64 };
    ctx.store_local(name, new, PhpType::Float, Some(expr.span));
    if post {
        old
    } else {
        ctx.load_local(name, Some(expr.span))
    }
}

/// Resolves the result type of a builtin reached through a resolved static callable binding.
///
/// A callable binding has already collapsed the source argument list into lowered operands, so the
/// registry descriptor is consulted with an empty AST argument list; builtins whose result type is
/// argument-VALUE dependent therefore fall back to the typed runtime target's
/// representation-safe layout instead of the broad declared `returns` type. Without this, a
/// container-returning builtin invoked as `$f = 'array_slice'; $f($a, 1, 2)` reached the backend
/// typed `mixed` while a direct `array_slice($a, 1, 2)` call reached it typed `array<mixed>`.
///
/// The checker's per-span result map is deliberately NOT consulted here. On this path `span`
/// identifies the DISPATCHING expression — `call_user_func(...)` or `$f(...)` — not this builtin,
/// so the type recorded there is the dispatcher's own result. When the checker cannot resolve the
/// callback statically (a variable holding the name, which constant propagation only turns into a
/// literal after checking) that entry is `call_user_func()`'s runtime-opaque `mixed`, and adopting
/// it labels a raw scalar/array return as a boxed Mixed cell: `$g = 'array_reverse';
/// call_user_func($g, [1, 2, 3])` printed `bool(true)`, and the same shape over a `bool`-returning
/// builtin crashed on the boxed-cell dereference.
fn static_callable_builtin_result_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
    span: Span,
) -> PhpType {
    // The checker's result for THIS span is authoritative here just as it is for a builtin
    // written out at its own call site: a first-class callable to a builtin resolves to a direct
    // call, so the site has its own argument types and the contract has already answered from
    // them. Passing `None` took the DECLARED return type instead, which for any builtin whose
    // result depends on its arguments is a different type -- `array_slice($assoc, 1, 2)` returns
    // an associative array where the declaration says `array`, and lowering then refused the
    // call outright (issue #1092).
    //
    // Read from the FIRST-CLASS map, never from `builtin_call_types`. That one is keyed by the
    // span of whatever call the checker was inferring, and for `call_user_func($f, ...)` that is
    // the OUTER call: reading it here handed a `bool`-returning builtin's raw result the outer
    // call's `Mixed` label, and the consumer dereferenced it as a pointer.
    let checked = if span.line != 0 {
        ctx.first_class_builtin_call_types
            .get(&span)
            .map(|checked| normalize_value_php_type(checked.clone()))
    } else {
        None
    };
    resolve_registry_builtin_result_type(ctx, name, &[], operands, span, checked)
        .unwrap_or_else(|| call_return_type(ctx, name, operands))
}

/// Lowers `isset($object->declaredProperty)` for a DECLARED (typed) property slot.
///
/// PHP answers `false` for a typed property that is uninitialized — either because it
/// never got a value or because `unset()` removed it — WITHOUT raising the
/// "must not be accessed before initialization" error that a plain read raises. The
/// slot probe therefore runs first, and the ordinary null-check read is only reached
/// on the initialized branch.
fn lower_initialized_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let uninitialized_block = ctx
        .builder
        .create_named_block("isset.property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("isset.property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.property.merge", Vec::new());
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    // The READ arm hands the receiver to `lower_property_get_from_value`, which consumes it. This
    // arm answers without reading, so an OWNING receiver has nowhere to go and leaked one object
    // per call: `isset(mk()->p)` three times reported `allocs=3 frees=0`, where the same program
    // with an INITIALIZED property closed at 3/3.
    //
    // `value_is_owning_temporary` is the gate, the same one the nullsafe chain uses where it
    // diverts past whatever would have consumed its receiver. A type-gated release is NOT enough:
    // a `?C` parameter represents as a boxed Mixed and passes that test while being BORROWED, so
    // releasing it here freed the receiver the next statement still needed —
    // `isset($c->p); $c->p ??= new P();` died with `Attempt to assign property "p" on null`,
    // measured before this gate was added.
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(arg.span));
    }
    let false_value = emit_bool_literal(ctx, false, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = property_access::lower_initialized_slot_read(ctx, object, property, arg);
    let is_set = emit_builtin_call_value(
        ctx,
        "isset",
        vec![read_value.value],
        PhpType::Int,
        arg.span,
        None,
    );
    let is_set = ctx.truthy_consuming(is_set, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, is_set, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, arg.span)
}

/// Lowers `isset(S::$s)` for a DECLARED (typed) STATIC property slot.
///
/// The static twin of `lower_initialized_property_isset`, and needed for the same reason: a
/// typed static property starts uninitialized, PHP answers `false` there without raising, and
/// the ordinary static read carries a fatal guard the lowering could not route around until
/// `Op::StaticPropInitialized` existed.
fn lower_initialized_static_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    if let Some(error) = crate::ir_lower::property_default_errors::for_receiver(ctx, receiver, arg.span) {
        return error;
    }
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let uninitialized_block = ctx
        .builder
        .create_named_block("isset.static_property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("isset.static_property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.static_property.merge", Vec::new());
    let data = crate::ir_lower::stmt::static_property_ref_data(ctx, receiver, property);
    let initialized = ctx.emit_value(
        Op::StaticPropInitialized,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::StaticPropInitialized.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    let false_value = emit_bool_literal(ctx, false, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_static_property_get(ctx, receiver, property, arg);
    let is_set = emit_builtin_call_value(
        ctx,
        "isset",
        vec![read_value.value],
        PhpType::Int,
        arg.span,
        None,
    );
    let is_set = ctx.truthy_consuming(is_set, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, is_set, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, arg.span)
}

/// Lowers `empty($o->p)` for a DECLARED (typed) INSTANCE property slot.
///
/// The instance twin of `lower_initialized_static_property_empty`, and needed for the same
/// reason: an uninitialized typed slot IS empty in PHP, and the ordinary read that would find
/// that out raises instead. `empty($o->p)` on `class C { public int $p; }` died with
/// `Typed property C::$p must not be accessed before initialization` where PHP answers
/// `bool(true)`. Only the static path probed; this dispatch had no arm for the instance one.
///
/// The uninitialized arm releases an OWNING receiver for the same reason the isset twin does —
/// it answers without the read that would have consumed it — and gates that release on
/// `value_is_owning_temporary`, because a borrowed `?C` receiver represents as a boxed Mixed and
/// a type-gated release frees what the next statement still needs.
fn lower_initialized_property_empty(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    construct: &str,
    arg: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let uninitialized_block = ctx
        .builder
        .create_named_block("empty.property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("empty.property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("empty.property.merge", Vec::new());
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(arg.span));
    }
    let true_value = emit_bool_literal(ctx, true, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    // `empty()` needs the VALUE: php asks `__isset` and then `__get`, the `??` fetch.
    let read_value = property_access::lower_initialized_slot_read_in_mode(
        ctx,
        object,
        property,
        PropertyFetchMode::Coalesce,
        arg,
    );
    let empty_value = lazy_isset::lower_empty_of_probed_value(ctx, read_value, construct, arg);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, arg.span)
}

/// Lowers `empty(S::$s)` for a DECLARED (typed) STATIC property slot: an uninitialized slot is
/// empty, and saying so must not go through the read whose backend guard is fatal.
fn lower_initialized_static_property_empty(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    construct: &str,
    arg: &Expr,
) -> LoweredValue {
    if let Some(error) = crate::ir_lower::property_default_errors::for_receiver(ctx, receiver, arg.span) {
        return error;
    }
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let uninitialized_block = ctx
        .builder
        .create_named_block("empty.static_property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("empty.static_property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("empty.static_property.merge", Vec::new());
    let data = crate::ir_lower::stmt::static_property_ref_data(ctx, receiver, property);
    let initialized = ctx.emit_value(
        Op::StaticPropInitialized,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::StaticPropInitialized.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    let true_value = emit_bool_literal(ctx, true, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_static_property_get(ctx, receiver, property, arg);
    let empty_value = lazy_isset::lower_empty_of_probed_value(ctx, read_value, construct, arg);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, arg.span)
}

/// Chooses how to unset an undeclared name on an `#[AllowDynamicProperties]` class.
///
/// PHP consults `__unset()` for such a name only when the dynamic property is ABSENT at
/// the unset site, and removes the hash entry silently when it is present — a decision
/// that depends on runtime state. A class that declares `__unset()` therefore keeps the
/// explicit unsupported diagnostic instead of silently picking one of the two behaviors;
/// a class without `__unset()` can only ever take the removal path.
fn dynamic_property_unset_action(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> UnsetPropertyAction {
    if class_method_signature(ctx, class_name, &php_symbol_key("__unset")).is_some() {
        return UnsetPropertyAction::Fallback;
    }
    UnsetPropertyAction::RemoveDynamic
}


/// Replaces arguments whose declared-parameter binding is decided by their literal spelling.
///
/// Runs before any argument is lowered so a callable-name string never materializes as string
/// storage and a constant bound to `int`/`float` is emitted already coerced. Positional and
/// named arguments are both handled; a spread makes the positional mapping unknowable, so the
/// remaining arguments are left alone.
///
/// Returns `None` when nothing needed rewriting, which is the overwhelmingly common case.
fn rewrite_literal_param_bindings(sig: &FunctionSig, args: &[Expr]) -> Option<Vec<Expr>> {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let mut rewritten: Option<Vec<Expr>> = None;
    let mut positional_idx = 0usize;
    let mut positional_known = true;
    for (arg_idx, arg) in args.iter().enumerate() {
        let (param_idx, value) = match &arg.kind {
            ExprKind::Spread(_) => {
                positional_known = false;
                continue;
            }
            ExprKind::NamedArg { name, value } => {
                let Some(param_idx) = sig
                    .params
                    .iter()
                    .take(regular_param_count)
                    .position(|(param_name, _)| param_name == name)
                else {
                    continue;
                };
                (param_idx, value.as_ref())
            }
            _ => {
                if !positional_known {
                    continue;
                }
                let param_idx = positional_idx;
                positional_idx += 1;
                (param_idx, arg)
            }
        };
        if param_idx >= regular_param_count
            || !sig.declared_params.get(param_idx).copied().unwrap_or(false)
            || sig.ref_params.get(param_idx).copied().unwrap_or(false)
        {
            continue;
        }
        let Some((_, param_ty)) = sig.params.get(param_idx) else {
            continue;
        };
        let Some(bound) = crate::types::param_binding::rewrite_literal_param_binding(param_ty, value)
        else {
            continue;
        };
        let slots = rewritten.get_or_insert_with(|| args.to_vec());
        slots[arg_idx] = match &arg.kind {
            ExprKind::NamedArg { name, .. } => Expr::new(
                ExprKind::NamedArg {
                    name: name.clone(),
                    value: Box::new(bound),
                },
                arg.span,
            ),
            _ => bound,
        };
    }
    rewritten
}

/// Lowers one of PHP's six internal-array-pointer builtins as a cursor-slot operation.
///
/// The builtin is selected through the registry's typed
/// `BuiltinArgumentLowering::ArrayInternalPointer(op)` descriptor, never by matching the
/// PHP name here, so the six stay distinguishable as metadata all the way down.
///
/// A plain-local receiver keeps its internal pointer in a hidden `Int` frame slot beside
/// that local (`LoweringContext::array_pointer_cursor_slot`). A read (`key`/`current`)
/// loads that cursor and boxes the key/value at it; a seek (`next`/`prev`/`reset`/`end`)
/// first calls `ArrayPtrSeek`, stores the new cursor, and then boxes the selected value.
///
/// Other receiver expressions are evaluated exactly once and start from the first position.
/// By-reference property and element calls reach this function through the shared place
/// adapter, which presents a temporary plain local and writes its value back afterwards.
/// Read-only expression receivers and fresh call results need no persistent cursor, so a
/// call-local zero cursor preserves their temporary-value semantics.
///
/// Returns `None` only for named/spread arguments or a wrong argument count, leaving those
/// shapes to the ordinary call diagnostics.
fn lower_array_internal_pointer(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    let op = match crate::builtins::registry::lookup(&canonical)
        .map(|def| def.spec.semantics.argument_lowering)
    {
        Some(crate::builtins::semantics::BuiltinArgumentLowering::ArrayInternalPointer(op)) => op,
        _ => return None,
    };
    if args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let (container, cursor_slot, cursor) = match &args[0].kind {
        ExprKind::Variable(variable) => {
            let container = ctx.load_local(variable, Some(args[0].span));
            let cursor_slot = ctx.array_pointer_cursor_slot(variable);
            let cursor = ctx
                .builder
                .emit_load_local(cursor_slot, IrType::I64, PhpType::Int);
            (container, Some(cursor_slot), cursor)
        }
        _ => {
            let container = lower_expr(ctx, &args[0]);
            let cursor = ctx.builder.emit_const_i64(0);
            (container, None, cursor)
        }
    };
    let cursor = match op.seek_mode() {
        None => cursor,
        Some(mode) => {
            let mode = ctx.builder.emit_const_i64(mode);
            let moved = ctx.emit_value(
                Op::RuntimeCall,
                vec![container.value, cursor, mode],
                Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                    crate::ir::RuntimeFnId::ArrayPtrSeek,
                ))),
                PhpType::Int,
                effects_lookup::runtime_effects(),
                Some(expr.span),
            );
            if let Some(cursor_slot) = cursor_slot {
                ctx.builder.emit_store_local(cursor_slot, moved.value);
            }
            moved.value
        }
    };
    let target = if op.reads_key() {
        crate::ir::RuntimeFnId::ArrayPtrKey
    } else {
        crate::ir::RuntimeFnId::ArrayPtrValue
    };
    let result = ctx.emit_value(
        Op::RuntimeCall,
        vec![container.value, cursor],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(target),
        )),
        PhpType::Mixed,
        effects_lookup::runtime_effects(),
        Some(expr.span),
    );
    // A variable's load is owned too when its slot stores another representation (a local seeded
    // with a Mixed value, then assigned a list, is read as the list through an unboxing copy);
    // `release_if_owned` leaves a borrowed load alone.
    crate::ir_lower::ownership::release_if_owned(ctx, container, Some(args[0].span));
    Some(result)
}

pub(crate) use indexed_array_literals::ir_array_storage_type;
pub(crate) use assoc_array_literals::merge_ir_assoc_value_type;
pub(crate) use indexed_array_literals::merge_ir_indexed_element_type;
pub(in crate::ir_lower) use assignments::lower_conditional_non_local_null_coalesce_assignment;

/// Returns the plain local variable bound to a call's first declared parameter.
///
/// Reference-bound locals and hidden place temporaries are excluded because retyping their
/// storage would also require updating a caller, property, or array-element contract outside the
/// current frame.
fn first_parameter_plain_local(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<(String, Span)> {
    let receiver = args.iter().enumerate().find_map(|(index, arg)| {
        let (param_index, place) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (
                sig.params.iter().position(|(param, _)| param == name)?,
                value.as_ref(),
            ),
            _ => (index, arg),
        };
        (param_index == 0).then_some(place)
    })?;
    let ExprKind::Variable(name) = &receiver.kind else {
        return None;
    };
    if !ctx.has_local_slot(name) || ctx.is_ref_bound_local(name) {
        return None;
    }
    if name.starts_with("__eir_place") {
        return None;
    }
    Some((name.clone(), receiver.span))
}
