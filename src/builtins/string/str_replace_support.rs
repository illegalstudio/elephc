//! Purpose:
//! Shared checker, result-type, effect, and EIR lowering contract of `str_replace()` and
//! `str_ireplace()`, covering both the three-string fast path and PHP's array forms.
//!
//! Called from:
//! - The `str_replace` and `str_ireplace` builtin homes in this directory.
//!
//! Key details:
//! - Three operands that can only be scalar strings, with no `$count`, keep the typed
//!   `__rt_str_replace`/`__rt_str_ireplace` runtime call and its `string` result.
//! - Anything else — an operand that may hold an array, an object operand, or a `$count`
//!   argument — calls the `crate::str_replace_prelude` helpers: each operand is boxed into a
//!   `mixed` cell for the call and the box is released right after it.
//! - The result type depends on `$subject` alone, through one function shared by the checker
//!   hook and the EIR resolver: `string` when the subject cannot hold an array, otherwise the
//!   boxed `mixed` cell the helper returns (an array keyed like the subject, or a string).
//! - `$count` is a write-only by-reference output. The checker leaves it uninferred and
//!   requires a plain variable, because the lowering writes the total back through
//!   `store_operand_local`, which can name every variable storage kind but not a property
//!   or an array element.
//! - Callable wrappers cannot compose a prelude call, so `first_class_callable_sig` keeps
//!   the wrapper on the three-string runtime ABI (the string-replace arm of the runtime
//!   function's callable-signature refinement in `crate::ir::runtime_fn`).

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowerFn,
    BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements,
    BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemanticInput,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op, RuntimeCallTarget, RuntimeFnId, ValueId};
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

/// PHP's parameter names, in declaration order.
const PARAMETERS: [&str; 4] = ["search", "replace", "subject", "count"];

/// Builds the shared descriptor for one of the two replace builtins.
pub(super) const fn semantics(target: RuntimeFnId, lower: BuiltinLowerFn) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Shared(result_type),
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::Independent,
        requirements: BuiltinRequirements::Static(target.requirements()),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(target),
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "typed backend operation has no runtime-selected wrapper contract",
        ),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Returns whether a value of this type may be an array at run time.
fn may_hold_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Iterable => true,
        PhpType::Union(members) => members.iter().any(may_hold_array),
        _ => false,
    }
}

/// Returns whether the three-string runtime helper can take this operand as it is: a scalar
/// it string-converts itself, or a union of such scalars.
fn fits_runtime_fast_path(ty: &PhpType) -> bool {
    if may_hold_array(ty) {
        return false;
    }
    match ty {
        PhpType::Resource(_) | PhpType::Union(_) => true,
        _ => matches!(
            ty.codegen_repr(),
            PhpType::Str
                | PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::Void
                | PhpType::Never
                | PhpType::TaggedScalar
        ),
    }
}

/// The result type for a given `$subject` type.
pub(crate) fn result_type_for_subject(subject: &PhpType) -> PhpType {
    if may_hold_array(subject) {
        PhpType::Mixed
    } else {
        PhpType::Str
    }
}

/// Shared result resolver: `$subject` is the third normalized operand.
fn result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    input
        .arg_types
        .get(2)
        .map_or(PhpType::Str, result_type_for_subject)
}

/// The fast path is pure; the helper path converts operands (warnings, `__toString()`),
/// may throw, and writes `$count`.
fn effects(input: &BuiltinSemanticInput<'_>) -> Effects {
    if input.arg_types.len() == 3 && input.arg_types.iter().all(fits_runtime_fast_path) {
        Effects::empty()
    } else {
        Effects::all()
    }
}

/// Returns the expression behind a possibly named argument.
fn argument_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}

/// Returns the parameter an argument binds: its name when named, else by position.
fn parameter_name(arg: &Expr, index: usize) -> Option<String> {
    match &arg.kind {
        ExprKind::NamedArg { name, .. } => Some(php_symbol_key(name)),
        _ => PARAMETERS.get(index).map(|name| (*name).to_string()),
    }
}

/// Returns whether one source-order argument of `builtin` binds the `$count` output, so the
/// checker leaves it uninferred and types the variable `int` after the call.
pub(crate) fn is_count_output_argument(builtin: &str, arg: &Expr, index: usize) -> bool {
    matches!(php_symbol_key(builtin).as_str(), "str_replace" | "str_ireplace")
        && parameter_name(arg, index).as_deref() == Some("count")
}

/// Infers `$search`, `$replace` and `$subject`, requires a plain variable for `$count`
/// (left uninferred: it may not exist before the call), and returns the subject-driven
/// result type. A call that cannot stay on the runtime fast path marks the program as
/// needing the str_replace prelude helpers, so declaration reachability keeps them.
pub(super) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut subject = None;
    let mut needs_helpers = false;
    for (index, arg) in cx.args.iter().enumerate() {
        let parameter = parameter_name(arg, index);
        let value = argument_value(arg);
        if parameter.as_deref() == Some("count") {
            if !matches!(value.kind, ExprKind::Variable(_)) {
                return Err(CompileError::new(
                    value.span,
                    &format!(
                        "{}(): Argument #4 ($count) must be a plain variable; elephc cannot \
                         write the replacement count into a property or array element",
                        cx.name
                    ),
                ));
            }
            needs_helpers = true;
            continue;
        }
        let ty = cx.checker.infer_type(value, cx.env)?;
        needs_helpers |= !fits_runtime_fast_path(&ty) || matches!(value.kind, ExprKind::Spread(_));
        if parameter.as_deref() == Some("subject") {
            subject = Some(ty);
        }
    }
    if needs_helpers {
        cx.checker.string_replace_helpers = true;
    }
    Ok(subject.as_ref().map_or(PhpType::Str, result_type_for_subject))
}

/// Lowers `str_replace()`.
pub(super) fn lower_str_replace(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    lower(ctx, call, RuntimeFnId::StrReplace, false)
}

/// Lowers `str_ireplace()`.
pub(super) fn lower_str_ireplace(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    lower(ctx, call, RuntimeFnId::StrIreplace, true)
}

/// Picks the runtime fast path or the prelude helper path for one call.
///
/// A fourth operand is either the `$count` variable or the materialized `null` default of a
/// named call that omitted it; only a variable asks for the total.
///
/// Declaration reachability keeps the helpers only when the checker saw a call that needs
/// them (or the program names the builtin as a callable), and lowering can see a boxed
/// operand where the checker saw a string (a `global`, or a local captured by reference, is
/// stored boxed). Without the helpers such a call falls back to the runtime fast path, which
/// string-converts a boxed operand itself and returns the `string` the checker typed.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    target: RuntimeFnId,
    case_insensitive: bool,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let inputs = [call.operand(0)?, call.operand(1)?, call.operand(2)?];
    let count_target = call
        .operands
        .get(3)
        .copied()
        .filter(|operand| ctx.operand_is_variable(*operand));
    let span = Some(call.span);
    let input_types = inputs.map(|operand| ctx.value_php_type(operand));
    let helpers_declared = ctx.declares_function(crate::str_replace_prelude::MIXED_HELPER);
    let fast_path = count_target.is_none()
        && (input_types.iter().all(fits_runtime_fast_path) || !helpers_declared);
    if fast_path {
        return Ok(ctx.emit_runtime_call(
            RuntimeCallTarget::Function(target),
            inputs.to_vec(),
            PhpType::Str,
            target.effects(),
            span,
        ));
    }
    if !helpers_declared {
        return Err(BuiltinLoweringError::new(format!(
            "{}() needs the str_replace prelude helpers, which this program does not declare",
            call.name
        )));
    }
    let ci = const_bool(ctx, case_insensitive, span);
    check_operand_shapes(ctx, inputs[0], inputs[1], ci, span);
    let mut boxes = Vec::new();
    let mut operands = Vec::with_capacity(4);
    for operand in inputs {
        operands.push(boxed_operand(ctx, operand, &mut boxes, span));
    }
    operands.push(ci);
    let (helper, result_type) = match result_type_for_subject(&ctx.value_php_type(inputs[2])) {
        PhpType::Str => (crate::str_replace_prelude::STRING_HELPER, PhpType::Str),
        _ => (crate::str_replace_prelude::MIXED_HELPER, PhpType::Mixed),
    };
    let result = ctx.emit_user_call(helper, operands, result_type, span);
    for boxed in boxes {
        ctx.emit_void(Op::Release, vec![boxed], None, Op::Release.default_effects(), span);
    }
    if let Some(count_target) = count_target {
        store_count(ctx, count_target, call.span)?;
    }
    Ok(result)
}

/// Emits a boolean constant.
fn const_bool(ctx: &mut dyn BuiltinLoweringContext, value: bool, span: Option<Span>) -> ValueId {
    ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(value)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        span,
    )
    .value
}

/// Returns whether a value of this type is an array whatever happens at run time.
fn is_definitely_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(is_definitely_array),
        _ => false,
    }
}

/// Answers `is_array($operand)` as a boolean value: a constant when the static type decides
/// it, the runtime type predicate otherwise.
fn is_array_answer(
    ctx: &mut dyn BuiltinLoweringContext,
    operand: ValueId,
    span: Option<Span>,
) -> ValueId {
    let ty = ctx.value_php_type(operand);
    if is_definitely_array(&ty) {
        return const_bool(ctx, true, span);
    }
    if !may_hold_array(&ty) {
        return const_bool(ctx, false, span);
    }
    ctx.emit_value(
        Op::TypePredicate,
        vec![operand],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        span,
    )
    .value
}

/// Raises php-src's `TypeError` for a string `$search` with an array `$replace` before any
/// operand is boxed, so the throw strands no box. Skipped when the static types rule it out.
fn check_operand_shapes(
    ctx: &mut dyn BuiltinLoweringContext,
    search: ValueId,
    replace: ValueId,
    ci: ValueId,
    span: Option<Span>,
) {
    if is_definitely_array(&ctx.value_php_type(search))
        || !may_hold_array(&ctx.value_php_type(replace))
    {
        return;
    }
    let search_is_array = is_array_answer(ctx, search, span);
    let replace_is_array = is_array_answer(ctx, replace, span);
    ctx.emit_user_call(
        crate::str_replace_prelude::CHECK_HELPER,
        vec![search_is_array, replace_is_array, ci],
        PhpType::Bool,
        span,
    );
}

/// Returns `operand` as a `mixed` cell for a helper parameter, boxing it (and recording the
/// box for release after the call) unless it already uses boxed storage.
fn boxed_operand(
    ctx: &mut dyn BuiltinLoweringContext,
    operand: ValueId,
    boxes: &mut Vec<ValueId>,
    span: Option<Span>,
) -> ValueId {
    if ctx.value_php_type(operand).codegen_repr() == PhpType::Mixed {
        return operand;
    }
    let boxed = ctx
        .emit_value(
            Op::MixedBox,
            vec![operand],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            span,
        )
        .value;
    boxes.push(boxed);
    boxed
}

/// Reads the helper's total back and assigns it to the `$count` variable.
fn store_count(
    ctx: &mut dyn BuiltinLoweringContext,
    count_target: ValueId,
    span: Span,
) -> Result<(), BuiltinLoweringError> {
    let read = ctx
        .emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(1)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(span),
        )
        .value;
    let unused = ctx
        .emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(0)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(span),
        )
        .value;
    let total = ctx.emit_user_call(
        crate::str_replace_prelude::COUNT_HELPER,
        vec![read, unused],
        PhpType::Int,
        Some(span),
    );
    if ctx.store_operand_local(count_target, total.value, PhpType::Int, Some(span)) {
        return Ok(());
    }
    Err(BuiltinLoweringError::new(
        "str_replace() $count must be a variable the lowering can store into",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a subject that may hold an array widens the result past `string`.
    #[test]
    fn result_type_follows_the_subject() {
        assert_eq!(result_type_for_subject(&PhpType::Str), PhpType::Str);
        assert_eq!(result_type_for_subject(&PhpType::Int), PhpType::Str);
        assert_eq!(
            result_type_for_subject(&PhpType::Union(vec![PhpType::Str, PhpType::Void])),
            PhpType::Str
        );
        assert_eq!(
            result_type_for_subject(&PhpType::Array(Box::new(PhpType::Str))),
            PhpType::Mixed
        );
        assert_eq!(result_type_for_subject(&PhpType::Mixed), PhpType::Mixed);
        assert_eq!(result_type_for_subject(&PhpType::php_array()), PhpType::Mixed);
    }

    /// The runtime fast path takes scalars only; arrays, mixed cells, and objects do not fit.
    #[test]
    fn fast_path_operands_are_scalar() {
        assert!(fits_runtime_fast_path(&PhpType::Str));
        assert!(fits_runtime_fast_path(&PhpType::Int));
        assert!(fits_runtime_fast_path(&PhpType::Union(vec![PhpType::Str, PhpType::Void])));
        assert!(!fits_runtime_fast_path(&PhpType::Array(Box::new(PhpType::Str))));
        assert!(!fits_runtime_fast_path(&PhpType::Mixed));
        assert!(!fits_runtime_fast_path(&PhpType::Object("Foo".to_string())));
    }
}
