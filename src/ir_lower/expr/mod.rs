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
    MixedNumericOp, Op, Ownership, Terminator, ValueId,
};
use crate::ir_lower::context::{
    value_ir_type, ClosureCapture, LoweredValue, LoweringContext, StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::ir_lower::function;
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method, Name};
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
mod nullsafe_chain;
mod builtin_graphs;
mod ref_place_args;
mod scalar_literals;
mod numeric_binary;
mod string_concat;
mod comparisons;
mod unary_logic;
mod lazy_branches;
mod pipe;
mod assignments;
mod function_calls;
use function_calls::resolve_registry_builtin_result_type;
mod eval_barriers;
mod lazy_isset;
mod native_isset;
mod callable_probes;
mod descriptor_invoke;
mod descriptor_args;
mod static_array_callbacks;
mod callable_tracking;
mod callable_resolution;
mod unset;
mod array_builtin_args;
mod builtin_special_args;
mod call_arg_coercion;
mod positional_spreads;
mod named_args;
mod named_spreads;
mod variadic_args;
mod call_return_types;
mod indexed_array_literals;
mod assoc_array_literals;
mod match_expr;
mod array_access;
mod array_access_types;
mod ternary_cast;
mod closures;
mod closure_calls;
mod descriptor_calls;
mod object_construction;
mod property_access;
mod property_fetch_for_write;
mod method_calls;
mod reflection_class_calls;
mod reflection_method_calls;
mod reflection_property_calls;
mod reflection_filters;
mod reflection_constructors;
mod reflection_static_properties;
mod reflection_new_instance;
mod nullable_method_calls;
mod method_metadata;
mod static_method_calls;
mod scoped_values;
mod generators;
mod instanceof_coercions;
mod merge_temps;

use scalar_literals::*;
use numeric_binary::*;
use string_concat::*;
use comparisons::*;
use unary_logic::*;
use lazy_branches::*;
use pipe::*;
use assignments::*;
use function_calls::*;
use eval_barriers::*;
use lazy_isset::*;
use native_isset::*;
use callable_probes::*;
use descriptor_invoke::*;
use descriptor_args::*;
use static_array_callbacks::*;
use callable_tracking::*;
use callable_resolution::*;
use unset::*;
use array_builtin_args::*;
use builtin_special_args::*;
use call_arg_coercion::*;
use positional_spreads::*;
use named_args::*;
use named_spreads::*;
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
use method_calls::*;
use reflection_class_calls::*;
use reflection_method_calls::*;
use reflection_property_calls::*;
use reflection_filters::*;
use reflection_constructors::*;
use reflection_static_properties::*;
use reflection_new_instance::*;
use nullable_method_calls::*;
use method_metadata::*;
use static_method_calls::*;
use scoped_values::*;
use generators::*;
use instanceof_coercions::*;
use merge_temps::*;
use builtin_graphs::*;

pub(crate) use callable_resolution::{
    instance_callable_object_class, is_bound_closure_assignment_shape,
    lower_bound_closure_for_assignment,
};
pub(crate) use assignments::lower_dynamic_property_array_push;
pub(crate) use builtin_graphs::{
    lower_array_end_from_value, lower_constant_from_name_value, lower_get_object_vars_from_value,
};
pub(crate) use callable_tracking::{
    lower_callable_array_for_assignment, reflection_arg_array_binding_for_expr,
    reflection_class_binding_for_expr, reflection_function_binding_for_expr,
    reflection_method_binding_for_expr, reflection_property_binding_for_expr,
    static_callable_binding_for_expr,
};
#[allow(unused_imports)]
pub(crate) use callable_tracking::LoweredCallableArrayAssignment;
pub(crate) use closures::{body_contains_eval_call, lower_closure_for_assignment};
pub(crate) use indexed_array_literals::{
    array_literal_type_for_ir, lower_array_literal_with_expected_type,
};
pub(crate) use array_access::{
    array_access_element_result_type, index_expr_key_type,
    lower_array_access_from_lowered_receiver, lower_by_ref_foreach_element_source,
};
pub(crate) use array_access_types::type_satisfies_array_access_for_ir;
pub(crate) use instanceof_coercions::coerce_to_int_at_span;
pub(crate) use merge_temps::emit_bool_literal;
pub(crate) use property_access::{
    lower_ref_assign_array_elem, lower_ref_assign_call, lower_ref_assign_property,
};
pub(crate) use property_fetch_for_write::lower_by_ref_foreach_property_source;
pub(crate) use string_concat::string_op_uses_scratch_storage;
pub(super) use assoc_array_literals::{
    array_access_expr_value_type_for_ir, method_call_expr_type_for_ir,
    property_access_expr_type_for_ir,
};
use call_return_types::{call_return_type, eir_user_function_return_type};
pub(super) use merge_temps::coerce_container_to_mixed_payload;
pub(super) use nullable_method_calls::lower_dynamic_method_call_with_receiver;
pub(super) use static_method_calls::static_method_call_expr_type_for_ir;

/// Lowers an expression and returns its EIR value.
pub(crate) fn lower_expr(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
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
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => lower_method_call(ctx, object, method, args, Op::MethodCall, expr),
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
        ExprKind::NewScopedObject { receiver, args } => lower_new_scoped_object(ctx, receiver, args, expr),
        ExprKind::MagicConstant(kind) => lower_magic_constant(ctx, kind, expr),
        ExprKind::Yield { key, value } => lower_yield(ctx, key.as_deref(), value.as_deref(), expr),
        ExprKind::YieldFrom(inner) => lower_yield_from(ctx, inner, expr),
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
    resolve_registry_builtin_result_type(ctx, name, &[], operands, span, None)
        .unwrap_or_else(|| call_return_type(ctx, name, operands))
}

/// Lowers `isset($object->declaredProperty)` for a DECLARED (typed) property slot.
///
/// PHP answers `false` for a typed property that is uninitialized — either because it
/// never got a value or because `unset()` removed it — WITHOUT raising the
/// "must not be accessed before initialization" error that a plain read raises. The
/// slot probe therefore runs first, and the ordinary null-check read is only reached
/// on the initialized branch.
#[allow(dead_code)]
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
    let read_value = lower_property_get_from_value(ctx, object, property, Op::PropGet, arg);
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
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
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
#[allow(dead_code)]
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
    let read_value = lower_property_get_from_value(ctx, object, property, Op::PropGet, arg);
    let construct_name = ctx.intern_function_name(construct);
    let empty_value = ctx.emit_value(
        Op::LanguageConstructCall,
        vec![read_value.value],
        Some(Immediate::Data(construct_name)),
        PhpType::Bool,
        effects_lookup::language_construct_effects(construct),
        Some(arg.span),
    );
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
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
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
    let construct_name = ctx.intern_function_name(construct);
    let empty_value = ctx.emit_value(
        Op::LanguageConstructCall,
        vec![read_value.value],
        Some(Immediate::Data(construct_name)),
        PhpType::Bool,
        effects_lookup::language_construct_effects(construct),
        Some(arg.span),
    );
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

/// Lowers `array_splice()` operands, promoting a typed receiver whose `$replacement` cannot fit.
///
/// PHP has no per-array element type, so `$a = [1, 2, 3]; array_splice($a, 1, 1, ["x"])` simply
/// leaves `[1, "x", 3]`. elephc types an indexed array at its payload slot, so the promotion has
/// to reach the receiver LOCAL: `__rt_array_to_mixed` re-boxes every live payload and the slot's
/// storage type widens to `array<mixed>`, which is the representation the boxed insert helper
/// writes into. Without it the backend would have to store a string pointer/length pair in an
/// 8-byte integer slot, which is why the untyped case used to be an explicit `unsupported`
/// diagnostic instead of a wrong answer.
fn lower_array_splice_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let mut operands = if crate::types::call_args::has_named_args(args) {
        lower_args_with_signature(ctx, sig, args)
    } else {
        lower_positional_builtin_args_with_signature(ctx, sig, args)
    };
    widen_array_splice_receiver_for_replacement(ctx, sig, args, &mut operands);
    operands
}

/// Promotes an `array_splice()` receiver local to `array<mixed>` when `$replacement` retypes it.
///
/// Runs after the operands are lowered because the decision needs both the receiver's slot
/// element type and the replacement's EIR type, and the conversion itself re-reads the receiver
/// local so it observes any mutation the later arguments performed.
fn widen_array_splice_receiver_for_replacement(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    operands: &mut [crate::ir::ValueId],
) {
    let Some(sig) = sig else {
        return;
    };
    let Some(replacement) = operands.get(3).copied() else {
        return;
    };
    let Some((name, span)) = array_splice_receiver_local(ctx, sig, args) else {
        return;
    };
    let PhpType::Array(elem_ty) = ctx.local_type(&name).codegen_repr() else {
        return;
    };
    let elem_ty = elem_ty.codegen_repr();
    if elem_ty == PhpType::Mixed {
        return;
    }
    let replacement_ty = ctx.builder.value_php_type(replacement).codegen_repr();
    if array_splice_replacement_fits_receiver(&elem_ty, &replacement_ty) {
        return;
    }
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let local = ctx.load_local(&name, Some(span));
    let converted = ctx.emit_value(
        Op::ArrayToMixed,
        vec![local.value],
        None,
        array_ty.clone(),
        Op::ArrayToMixed.default_effects(),
        Some(span),
    );
    ctx.store_mutated_local(&name, converted, array_ty, Some(span));
    operands[0] = ctx.load_local(&name, Some(span)).value;
}

/// Returns the plain local variable bound to `array_splice()`'s by-reference receiver.
///
/// Two receiver shapes are deliberately excluded even though they name a local. A by-reference
/// parameter and a `&$x` binding share storage with a caller slot this function cannot retype,
/// and the hidden `__eir_place` temporary of the property/element rewrite is written back into a
/// place whose declared element type is equally out of reach. Widening either would publish
/// boxed `Mixed` cells through a slot still described as `array<int>`, so both keep the
/// backend's explicit diagnostic instead.
fn array_splice_receiver_local(
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

/// Reports whether a `$replacement` can be written into the receiver's existing payload slots.
///
/// Mirrors the shapes the backend's `SpliceReplacement` classifier accepts, so a call this
/// predicate passes never reaches the `unsupported` arm: an omitted/null/empty replacement
/// inserts nothing, an array of the receiver's own element type is copied verbatim, an array of
/// boxed `Mixed` cells is read back as plain integers for an `int`/`bool` receiver (the shape
/// `[$x + 1]` produces), and a bare scalar of the element type becomes a one-element insertion.
fn array_splice_replacement_fits_receiver(elem_ty: &PhpType, replacement_ty: &PhpType) -> bool {
    if matches!(replacement_ty, PhpType::Void | PhpType::Never) {
        return true;
    }
    if let PhpType::Array(inner) = replacement_ty {
        let inner = inner.codegen_repr();
        if matches!(inner, PhpType::Void | PhpType::Never) {
            return true;
        }
        if &inner == elem_ty {
            return true;
        }
        return inner == PhpType::Mixed && matches!(elem_ty, PhpType::Int | PhpType::Bool);
    }
    replacement_ty == elem_ty
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

/// Lowers `new $class(...)` as a class-name dispatch chain when the call shape needs
/// per-class argument planning.
///
/// Returns `None` when the raw operand list is already correct for every class the
/// runtime dispatch could select, which keeps today's operand ABI for the common
/// exact-arity positional call. Otherwise the class name is evaluated once into a hidden
/// temporary, compared case-insensitively against each candidate class, and each matching
/// branch lowers a fixed-class `new` so named arguments, defaults for omitted optional
/// parameters, and runtime spreads all go through `plan_call_args`. The final `else`
/// branch keeps the generic dynamic-new opcode so runtime-registry classes, the eval
/// bridge, and PHP's class-not-found fatal behave exactly as before.
fn lower_new_dynamic_planned_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    name_expr: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let candidates = dynamic_new_planned_candidate_classes(ctx, args);
    if candidates.is_empty() {
        return None;
    }

    let name_value = lower_expr(ctx, name_expr);
    let name_type = match ctx.builder.value_php_type(name_value.value).codegen_repr() {
        PhpType::Str => PhpType::Str,
        _ => PhpType::Mixed,
    };
    let name_temp = ctx.declare_owned_hidden_temp(name_type.clone());
    store_value_into_temp(ctx, &name_temp, name_type.clone(), name_value, expr.span);
    let name_var = Expr::new(ExprKind::Variable(name_temp.clone()), name_expr.span);

    let result_temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let merge = ctx
        .builder
        .create_named_block("new.dynamic.planned.merge", Vec::new());

    for class_name in &candidates {
        let match_block = ctx
            .builder
            .create_named_block("new.dynamic.planned.match", Vec::new());
        let next_block = ctx
            .builder
            .create_named_block("new.dynamic.planned.next", Vec::new());
        let condition = dynamic_new_class_name_match_expr(
            &name_var,
            class_name,
            name_type == PhpType::Str,
            name_expr.span,
        );
        let condition = lower_expr(ctx, &condition);
        let condition = coerce_to_int_at_span(ctx, condition, Some(expr.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value,
            then_target: match_block,
            then_args: Vec::new(),
            else_target: next_block,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(match_block);
        let class = Name::unqualified(class_name.clone());
        let object = lower_new_object(ctx, &class, args, expr);
        store_value_into_temp(ctx, &result_temp, PhpType::Mixed, object, expr.span);
        branch_to(ctx, merge);

        ctx.builder.position_at_end(next_block);
    }

    let name_value = ctx.load_local(&name_temp, Some(expr.span));
    let fallback = lower_new_dynamic_generic(ctx, name_value, args, expr);
    store_value_into_temp(ctx, &result_temp, PhpType::Mixed, fallback, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.clear_owned_hidden_temp(&name_temp, Some(expr.span));
    Some(take_owned_temp(ctx, &result_temp, expr.span))
}

/// Builds the case-insensitive class-name test for one dispatch-chain branch.
///
/// PHP class names are case-insensitive, so the comparison goes through `strcasecmp`.
/// A class-name expression that is not statically a string is guarded by `is_string`
/// first: `new $object()` and other non-string operands must keep falling through to the
/// generic dynamic-new opcode instead of being stringified here.
fn dynamic_new_class_name_match_expr(
    name_var: &Expr,
    class_name: &str,
    name_is_string: bool,
    span: Span,
) -> Expr {
    let compare = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("strcasecmp"),
                    args: vec![
                        name_var.clone(),
                        Expr::new(ExprKind::StringLiteral(class_name.to_string()), span),
                    ],
                },
                span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
        },
        span,
    );
    if name_is_string {
        return compare;
    }
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("is_string"),
                    args: vec![name_var.clone()],
                },
                span,
            )),
            op: BinOp::And,
            right: Box::new(compare),
        },
        span,
    )
}

/// Returns the classes whose constructor would receive different arguments than the raw
/// dynamic-new operand list, in deterministic order.
///
/// Only classes that EIR can construct as a fixed class are considered: compiler-internal
/// classes, runtime-managed builtin classes, and classes without an emitted constructor body
/// keep the generic opcode. A class qualifies when the shared call-argument rules accept the
/// call for its constructor, `lower_args_with_signature` is known to resolve it to exactly one
/// operand per declared parameter, *and* the result differs from the raw source arguments —
/// that is exactly the set of classes the generic opcode's exact-arity candidate match would
/// otherwise silently skip or feed in source order.
fn dynamic_new_planned_candidate_classes(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Vec<String> {
    if args.is_empty() {
        return Vec::new();
    }
    let constructor_key = php_symbol_key("__construct");
    let mut candidates = ctx
        .classes
        .iter()
        .filter(|(class_name, class_info)| {
            dynamic_new_class_is_planning_candidate(
                ctx,
                class_name,
                class_info,
                &constructor_key,
                args,
            )
        })
        .map(|(class_name, _)| class_name.clone())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

/// Returns true when a dynamic `new` should construct `class_name` through a fixed-class
/// branch instead of the generic dynamic-new opcode.
fn dynamic_new_class_is_planning_candidate(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    constructor_key: &str,
    args: &[Expr],
) -> bool {
    if class_info.is_abstract || ctx.enums.contains_key(class_name) {
        return false;
    }
    if php_symbol_key(class_name).starts_with("__elephc") {
        return false;
    }
    if crate::codegen_support::dynamic_new::known_dynamic_new_builtin_class_names()
        .contains(&class_name)
    {
        return false;
    }
    // Runtime-registered builtin classes carry no source constructor body, so a fixed-class
    // `ObjectNew` branch would reference a method symbol EIR never emits.
    if class_info.declaration_span == Span::dummy() {
        return false;
    }
    if !class_info
        .method_decls
        .iter()
        .any(|method| php_symbol_key(&method.name) == constructor_key && method.has_body)
    {
        return false;
    }
    let Some(sig) = class_info.methods.get(constructor_key) else {
        return false;
    };
    if sig.variadic.is_some() {
        return false;
    }
    if !dynamic_new_args_need_planning(sig, args) {
        return false;
    }
    dynamic_new_args_lower_to_exact_arity(ctx, sig, args)
}

/// Returns true when a constructor signature would reshape the source argument list.
///
/// The generic dynamic-new opcode forwards operands positionally and only selects a
/// candidate whose constructor arity matches the operand count exactly, so any named
/// argument, spread, or omitted optional parameter needs the planned path.
fn dynamic_new_args_need_planning(sig: &FunctionSig, args: &[Expr]) -> bool {
    crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
        || sig.params.len() != args.len()
}

/// Returns true when `lower_args_with_signature` resolves this call to exactly one operand
/// per declared constructor parameter.
///
/// The fixed-class `ObjectNew` opcode is arity-exact, and the shared argument lowering falls
/// back to a raw source-order operand list for call shapes it cannot resolve (dynamic
/// associative spreads, multiple spreads, spreads that do not feed the parameter tail). Those
/// shapes must keep the generic dynamic-new opcode rather than produce a mis-arity call.
fn dynamic_new_args_lower_to_exact_arity(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> bool {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    if crate::types::call_args::has_named_args(args) {
        let Ok(plan) =
            crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
                sig,
                args,
                args[0].span,
                regular_param_count,
                false,
                true,
                &assoc_spread_sources(ctx, args),
            )
        else {
            return false;
        };
        return !plan.has_spread_args() && plan.regular_args.len() == sig.params.len();
    }
    if args.iter().any(is_spread_arg) {
        return single_trailing_indexed_spread_arg(ctx, args)
            .is_some_and(|spread_idx| spread_idx <= regular_param_count);
    }
    if args.len() > sig.params.len() {
        return false;
    }
    (args.len()..sig.params.len()).all(|index| {
        sig.defaults
            .get(index)
            .is_some_and(|default| default.is_some())
    })
}

/// Lowers one of PHP's six internal-array-pointer builtins as a cursor-slot operation.
///
/// The builtin is selected through the registry's typed
/// `BuiltinArgumentLowering::ArrayInternalPointer(op)` descriptor, never by matching the
/// PHP name here, so the six stay distinguishable as metadata all the way down.
///
/// The receiver's internal pointer is a hidden `Int` frame slot beside the array local
/// (`LoweringContext::array_pointer_cursor_slot`). A read (`key`/`current`) loads that
/// cursor and boxes the key/value at it; a seek (`next`/`prev`/`reset`/`end`) first calls
/// `ArrayPtrSeek` to compute the new cursor, stores it back, and then boxes the value at
/// the new position — the same two-step shape PHP's own implementations use.
///
/// Returns `None` for anything this path cannot own (named/spread arguments, a wrong
/// argument count, or a receiver that is not a plain variable) so the generic builtin path
/// still runs. The checker has already rejected those shapes with a source-level
/// diagnostic (`crate::builtins::array::internal_pointer`), so reaching the generic path
/// in a successful compile is not possible.
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
    let ExprKind::Variable(variable) = &args[0].kind else {
        return None;
    };
    let variable = variable.clone();
    let container = ctx.load_local(&variable, Some(args[0].span));
    let cursor_slot = ctx.array_pointer_cursor_slot(&variable);
    let cursor = ctx
        .builder
        .emit_load_local(cursor_slot, IrType::I64, PhpType::Int);
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
            ctx.builder.emit_store_local(cursor_slot, moved.value);
            moved.value
        }
    };
    let target = if op.reads_key() {
        crate::ir::RuntimeFnId::ArrayPtrKey
    } else {
        crate::ir::RuntimeFnId::ArrayPtrValue
    };
    Some(ctx.emit_value(
        Op::RuntimeCall,
        vec![container.value, cursor],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(target),
        )),
        PhpType::Mixed,
        effects_lookup::runtime_effects(),
        Some(expr.span),
    ))
}

/// Emits the generic runtime class-name dispatch for `new $class(...)`.
///
/// The class-name operand is already lowered so both the direct path and the
/// planned-dispatch fallback branch can share it.
fn lower_new_dynamic_generic(
    ctx: &mut LoweringContext<'_, '_>,
    name_value: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![name_value.value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(
        Op::DynamicObjectNewMixed,
        operands,
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewMixed.default_effects(),
        Some(expr.span),
    )
}

pub(crate) use indexed_array_literals::ir_array_storage_type;
pub(crate) use assoc_array_literals::merge_ir_assoc_value_type;
pub(crate) use indexed_array_literals::merge_ir_indexed_element_type;
