//! Purpose:
//! Function return storage coercion and container widening.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Coerces a value to the current function return storage type when needed.
pub(super) fn coerce_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    value_expr: Option<&Expr>,
    span: Option<Span>,
) -> LoweredValue {
    if let Some(value) = verify_declared_scalar_return(ctx, value, value_expr, span) {
        return value;
    }
    if let Some(value) = coerce_container_to_return_type(ctx, value, span) {
        return value;
    }
    if value.ir_type == ctx.return_type {
        return value;
    }
    match ctx.return_type {
        IrType::I64 => {
            if let Some(verified) = declared_int_return_boundary(ctx, value, span) {
                return verified;
            }
            coerce_return_scalar_source(ctx, value, span, coerce_to_int)
        }
        IrType::F64 => coerce_return_scalar_source(ctx, value, span, coerce_to_float),
        IrType::Str => coerce_return_scalar_source(ctx, value, span, coerce_to_string),
        IrType::TaggedScalar => {
            coerce_return_scalar_source(ctx, value, span, coerce_to_tagged_scalar)
        }
        IrType::Heap(_) if ctx.return_php_type.codegen_repr() == PhpType::Mixed => {
            ctx.box_value_as_mixed(value, ctx.return_php_type.clone(), span)
        }
        IrType::Heap(_) => ctx.emit_value(
            Op::RuntimeCall,
            vec![value.value],
            None,
            ctx.return_php_type.clone(),
            effects_lookup::runtime_effects(),
            span,
        ),
        IrType::Void => value,
    }
}

/// Verifies a value against a DECLARED scalar return contract through the return-coercion
/// prelude helper, when its static type does not already prove the contract.
///
/// Covers every purely scalar declaration — `bool`, `int`, `float`, `string`, their nullable
/// forms and scalar unions like `int|string` or `string|false` — for a source that is `mixed`,
/// a union, `?int` or another scalar representation. The helper applies PHP's rules for the
/// return statement's own file mode: an exact member passes, `strict_types` only widens int to
/// float, coercive mode converts in php-src's int > float > string > bool order, and anything
/// else throws `TypeError` ("f(): Return value must be of type T, V returned"). Its `mixed`
/// result already holds a value of a declared member, so narrowing it to the return storage is
/// a plain cast (or, for a union, no change at all).
///
/// The plain storage coercions this replaces converted blindly: `mixed` into `: bool` was an
/// int cast (`"x"` came back `false`), `null` and arrays never threw, `strict_types` was
/// ignored, a union return passed any value through untouched, and `mixed` into `?int` failed
/// codegen outright.
///
/// Statically exact sources keep the direct path, as do an int widening into a `float` slot
/// and a coercive `int` return from `mixed`/`float`, which `declared_int_return_boundary`
/// verifies with the same rules and no call. INFERRED returns are never verified.
fn verify_declared_scalar_return(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    value_expr: Option<&Expr>,
    span: Option<Span>,
) -> Option<LoweredValue> {
    use crate::return_coerce_prelude as rc;
    if !ctx.return_type_is_declared {
        return None;
    }
    // Compiler-built bodies (builtin class methods, synthetic helpers) carry no source span and
    // return exactly their declared type; only PHP source — the program and the PHP-written
    // preludes — is verified, and only it is what the helper's injection detects.
    if !span.is_some_and(Span::is_from_source) {
        return None;
    }
    let mut mask = rc::scalar_mask(&ctx.return_php_type)?;
    if ctx.return_literal_true {
        mask = rc::literal_true_mask(mask);
    }
    let source = ctx.builder.value_php_type(value.value);
    if rc::statically_satisfies(&source, mask) {
        return None;
    }
    if source == PhpType::Int && ctx.return_type == IrType::F64 {
        return None;
    }
    let strict = crate::source::current_strict_types();
    // A numeric source (int/float arithmetic, a float) can only hold an int or a float, which
    // the dedicated boundary op verifies in both modes without a call: the overflow float of
    // `$a + 1` throws "float returned" exactly where php-src does. Strings never take this path,
    // so its numeric-string coercion cannot saturate (`"1e100"` throws through the helper).
    let numeric = source.codegen_repr() == PhpType::Float
        || source.is_int_float_union()
        || value_expr.is_some_and(|expr| expr_is_numeric(ctx, expr));
    // A plain `int` declaration from any `mixed` source also takes the boundary op: it verifies
    // every tag inline (numeric strings through php-src's `is_numeric_string` classifier, so an
    // out-of-range spelling throws) and names an object's class, exactly like the helper.
    if mask == rc::MASK_INT && (numeric || source.codegen_repr() == PhpType::Mixed) {
        return Some(emit_int_return_boundary(ctx, value, span, strict, mask));
    }
    if numeric {
        if mask == rc::MASK_FLOAT {
            // int widens to float in both modes and a float passes, so the cast is exact.
            return Some(coerce_return_scalar_source(ctx, value, span, coerce_to_float));
        }
        if mask == rc::MASK_INT | rc::MASK_NULL && ctx.return_type == IrType::TaggedScalar {
            let verified = emit_int_return_boundary(ctx, value, span, strict, mask);
            return Some(coerce_to_tagged_scalar(ctx, verified, span));
        }
    }
    if !ctx.functions.contains_key(rc::HELPER) {
        // Every pipeline injects the helper and reachability keeps it for each scalar return
        // annotation; a missing declaration is a pipeline bug, never a reason to convert
        // blindly.
        crate::ir_lower::diagnostics::refuse(
            span.unwrap_or_else(Span::dummy),
            "internal error: the return-type coercion helper was not injected",
        );
        return None;
    }
    // The value goes through a synthetic local and an ordinary call expression, as the
    // `(object)` cast does: the user-call path then boxes it for the `mixed` parameter, supplies
    // any hidden parameter the callee grew (`func_args` adds one when the program uses `eval`),
    // roots the arguments and releases their temporaries. The call gets a synthetic span, so no
    // span-keyed checker table (generic instantiations, builtin result types) can match it.
    let local_type = super::static_property_helpers::normalize_value_php_type(source);
    let temp = ctx.declare_synthetic_php_local(local_type.clone());
    ctx.store_local(&temp, value, local_type, span);
    let call_span = Span::synthetic();
    let prefix = format!(
        "{}(): Return value must be of type {}, ",
        ctx.owner_name(),
        rc::declared_type_name(mask)
    );
    let args = vec![
        Expr::new(ExprKind::Variable(temp), call_span),
        Expr::new(ExprKind::IntLiteral(mask as i64), call_span),
        Expr::new(ExprKind::BoolLiteral(strict), call_span),
        Expr::new(ExprKind::StringLiteral(prefix), call_span),
    ];
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::from(rc::HELPER.to_string()),
            args,
        },
        call_span,
    );
    let verified = lower_expr(ctx, &call);
    Some(narrow_verified_return(ctx, verified, span))
}

/// Returns whether an expression is int/float arithmetic over operands that are statically
/// numeric (literals, int/float/bool/`?int` locals, casts and nested arithmetic), so its value is
/// an int or a float and never a string, array or object (`null + 1` is `1`).
fn expr_is_numeric(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    use crate::parser::ast::{BinOp, CastType};
    match &expr.kind {
        ExprKind::BinaryOp { left, op, right } => {
            matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow)
                && numeric_operand(ctx, left)
                && numeric_operand(ctx, right)
        }
        ExprKind::Negate(inner) => numeric_operand(ctx, inner),
        ExprKind::Cast { target: CastType::Int | CastType::Float, .. } => true,
        _ => false,
    }
}

/// Returns whether one arithmetic operand is statically an int, float or bool.
fn numeric_operand(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) => true,
        ExprKind::Variable(name) => matches!(
            ctx.local_type(name).codegen_repr(),
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::TaggedScalar
        ),
        _ => expr_is_numeric(ctx, expr),
    }
}

/// Emits the dedicated `int` return boundary for the current function, carrying the return
/// statement's `strict_types` mode so strict returns reject floats and bools.
fn emit_int_return_boundary(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
    strict: bool,
    mask: u32,
) -> LoweredValue {
    let source = ctx.builder.value_php_type(value.value).codegen_repr();
    if source == PhpType::Int {
        return value;
    }
    let prefix = format!(
        "{}(): Return value must be of type {}, ",
        ctx.owner_name(),
        crate::return_coerce_prelude::declared_type_name(mask)
    );
    let data = ctx.intern_string(&prefix);
    let immediate = if strict {
        Immediate::ProfiledData { data, strict_php: true }
    } else {
        Immediate::Data(data)
    };
    let verified = ctx.emit_value(
        Op::ReturnBoundaryMixedToInt,
        vec![value.value],
        Some(immediate),
        PhpType::Int,
        Op::ReturnBoundaryMixedToInt.default_effects(),
        span,
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    verified
}

/// Narrows the helper's verified `mixed` result to the current return storage.
///
/// The value already holds a declared member, so each cast is exact: a scalar slot takes the
/// cast of its own type, `?int` the tagged-scalar conversion (null stays null), and every
/// other union keeps the boxed value.
fn narrow_verified_return(
    ctx: &mut LoweringContext<'_, '_>,
    verified: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let declared = ctx.return_php_type.clone();
    match ctx.return_type {
        IrType::I64 => {
            let target = if declared.codegen_repr() == PhpType::Bool {
                PhpType::Bool
            } else {
                PhpType::Int
            };
            let cast = ctx.emit_value(
                Op::Cast,
                vec![verified.value],
                Some(Immediate::CastTarget(IrType::I64)),
                target,
                Op::Cast.default_effects(),
                span,
            );
            crate::ir_lower::ownership::release_if_owned(ctx, verified, span);
            cast
        }
        IrType::F64 => coerce_return_scalar_source(ctx, verified, span, coerce_to_float),
        IrType::Str => coerce_return_scalar_source(ctx, verified, span, coerce_to_string),
        IrType::TaggedScalar => {
            coerce_return_scalar_source(ctx, verified, span, coerce_to_tagged_scalar)
        }
        _ => verified,
    }
}

/// Verifies a dynamically-typed value against a DECLARED `int` return boundary.
///
/// A declared boundary carries PHP's coercive-mode verification: an in-range float
/// truncates, but a float outside the int range, a non-numeric string, null, or any
/// container throws `TypeError` — where the plain `coerce_to_int` path silently wrapped
/// the value. Applies only when the source type still needs a runtime decision (boxed
/// Mixed, or a raw F64 left by constant folding); statically-int sources keep the direct
/// path, and INFERRED int returns are untouched — an undeclared function must never throw
/// on its own return value.
fn declared_int_return_boundary(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    if !ctx.return_type_is_declared || ctx.return_php_type != PhpType::Int {
        return None;
    }
    if !matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Float
    ) {
        return None;
    }
    let prefix = format!("{}(): Return value must be of type int, ", ctx.owner_name());
    let data = ctx.intern_string(&prefix);
    let verified = ctx.emit_value(
        Op::ReturnBoundaryMixedToInt,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Int,
        Op::ReturnBoundaryMixedToInt.default_effects(),
        span,
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    Some(verified)
}

/// Coerces a return value and releases the old owning temporary when replaced.
pub(super) fn coerce_return_scalar_source(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
    coerce: fn(&mut LoweringContext<'_, '_>, LoweredValue, Option<Span>) -> LoweredValue,
) -> LoweredValue {
    let coerced = coerce(ctx, value, span);
    if coerced.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
}

/// Coerces an integer-or-null value into the two-word tagged-scalar return shape.
pub(super) fn coerce_to_tagged_scalar(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    if value.ir_type == IrType::TaggedScalar {
        return value;
    }
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Void
    ) {
        return ctx.emit_value(
            Op::ConstNull,
            Vec::new(),
            None,
            PhpType::TaggedScalar,
            Op::ConstNull.default_effects(),
            span,
        );
    }
    ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        PhpType::TaggedScalar,
        effects_lookup::runtime_effects(),
        span,
    )
}

/// Widens returned container payload storage to the current function return contract.
pub(super) fn coerce_container_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let return_ty = ctx.return_php_type.codegen_repr();
    let op = match (source_ty, return_ty.clone()) {
        (PhpType::Array(source_elem), PhpType::Array(return_elem))
            if source_elem.codegen_repr() != PhpType::Mixed
                && return_elem.codegen_repr() == PhpType::Mixed =>
        {
            Op::ArrayToMixed
        }
        (
            PhpType::AssocArray {
                value: source_value,
                ..
            },
            PhpType::AssocArray {
                value: return_value,
                ..
            },
        ) if source_value.codegen_repr() != PhpType::Mixed
            && return_value.codegen_repr() == PhpType::Mixed =>
        {
            Op::HashToMixed
        }
        (PhpType::Array(source_elem), PhpType::AssocArray { .. })
            if source_elem.as_ref() == &PhpType::Never =>
        {
            Op::ArrayToHash
        }
        _ => return None,
    };
    Some(ctx.emit_value(
        op,
        vec![value.value],
        None,
        return_ty,
        op.default_effects(),
        span,
    ))
}

/// Coerces a value to integer storage.
pub(super) fn coerce_to_int(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(
            Op::FToI,
            vec![value.value],
            None,
            PhpType::Int,
            Op::FToI.default_effects(),
            span,
        ),
        IrType::Str => ctx.emit_value(
            Op::StrToI,
            vec![value.value],
            None,
            PhpType::Int,
            Op::StrToI.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            PhpType::Int,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

/// Coerces a value to float storage.
pub(super) fn coerce_to_float(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::F64 => value,
        IrType::I64 => ctx.emit_value(
            Op::IToF,
            vec![value.value],
            None,
            PhpType::Float,
            Op::IToF.default_effects(),
            span,
        ),
        IrType::Str => ctx.emit_value(
            Op::StrToF,
            vec![value.value],
            None,
            PhpType::Float,
            Op::StrToF.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::F64)),
            PhpType::Float,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

/// Coerces a value to string storage.
pub(super) fn coerce_to_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::Str => value,
        IrType::I64 | IrType::TaggedScalar => ctx.emit_value(
            Op::IToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::IToStr.default_effects(),
            span,
        ),
        IrType::F64 => ctx.emit_value(
            Op::FToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::FToStr.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            PhpType::Str,
            Op::Cast.default_effects(),
            span,
        ),
    }
}


/// Re-applies a declared scalar parameter's coercion inside a body its override family boxed.
///
/// The family's shared call ABI passes the parameter as Mixed, so the caller no longer converts
/// it the way a call to the unboxed signature would. This applies the same conversion on entry:
/// `int` with PHP's coercive verification (a non-numeric string, null or container throws
/// `TypeError`), `string`, `float` and `bool` with the conversions a typed call site performs.
/// The local keeps its Mixed storage, now holding the coerced value.
pub(crate) fn coerce_boxed_family_param(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    declared: &PhpType,
    type_error_prefix: &str,
) {
    let value = ctx.load_local(name, None);
    let coerced = match declared.codegen_repr() {
        PhpType::Int => {
            let data = ctx.intern_string(type_error_prefix);
            ctx.emit_value(
                Op::ReturnBoundaryMixedToInt,
                vec![value.value],
                Some(Immediate::Data(data)),
                PhpType::Int,
                Op::ReturnBoundaryMixedToInt.default_effects(),
                None,
            )
        }
        PhpType::Float => coerce_to_float(ctx, value, None),
        PhpType::Str => coerce_to_string(ctx, value, None),
        PhpType::Bool => ctx.emit_value(
            Op::IsTruthy,
            vec![value.value],
            None,
            PhpType::Bool,
            Op::IsTruthy.default_effects(),
            None,
        ),
        _ => return,
    };
    let boxed = ctx.box_value_as_mixed(coerced, PhpType::Mixed, None);
    ctx.store_local(name, boxed, PhpType::Mixed, None);
}
