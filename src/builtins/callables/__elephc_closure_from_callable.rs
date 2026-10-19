//! Purpose:
//! Home of the internal `__elephc_closure_from_callable` builtin: PHP's
//! `$callable(...)` / `Closure::fromCallable($callable)` over a value only known at run time.
//!
//! Called from:
//! - The parser's `$var(...)` desugaring and the name resolver's non-literal
//!   `Closure::fromCallable()` rewrite.
//!
//! Key details:
//! - This home is the `$callable(...)` spelling (PHP throws `Error` for a non-callable); its
//!   `_type_error` sibling is `Closure::fromCallable()` (PHP throws `TypeError`). Both share
//!   `lower_closure_conversion` and differ only in the conversion mode immediate.
//! - An existing Closure is returned as the same descriptor (PHP returns the same object).
//! - Every other value is boxed and resolved through the AOT descriptor tables: function and
//!   `Class::method` names compiled into the program, `[$object, 'method']` / `['Class',
//!   'method']` arrays, and objects with `__invoke`. A name missing from those tables throws at
//!   run time; nothing aborts or silently yields a wrong Closure.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

/// `$callable(...)`: a non-callable value throws `Error`, as PHP's INIT_DYNAMIC_CALL does.
pub(crate) const MODE_FIRST_CLASS_CALLABLE: i64 = 1;
/// `Closure::fromCallable($callable)`: a non-callable value throws `TypeError`.
pub(crate) const MODE_FROM_CALLABLE: i64 = 2;
/// Mode flag: the operand is a box this conversion created, so a rejection must release it
/// before throwing (the `Release` emitted after the conversion only runs on success).
pub(crate) const MODE_OWNS_OPERAND_BOX: i64 = 4;

builtin! {
    contract: "__elephc_closure_from_callable",
    check: check_closure_conversion,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP
            .union(Effects::ALLOC_HEAP)
            .union(Effects::REFCOUNT_OP)
            .union(Effects::MAY_THROW),
        BuiltinResultOwnership::Fresh,
    ),
}

/// Infers the callable source; the result is always a Closure descriptor.
pub(crate) fn check_closure_conversion(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Callable)
}

/// Lowers `$callable(...)`.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    lower_closure_conversion(ctx, call, MODE_FIRST_CLASS_CALLABLE)
}

/// Lowers the conversion to `NormalizeCallable`, boxing typed sources so the runtime selector
/// can classify them and throw PHP's diagnostic instead of aborting.
pub(crate) fn lower_closure_conversion(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    mode: i64,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let source = call.operand(0)?;
    let source_type = ctx.value_php_type(source).codegen_repr();
    if source_type == PhpType::Callable {
        return Ok(ctx.emit_value(
            Op::NormalizeCallable,
            vec![source],
            None,
            PhpType::Callable,
            Op::NormalizeCallable.default_effects(),
            Some(call.span),
        ));
    }
    let boxed = !matches!(source_type, PhpType::Mixed | PhpType::Union(_));
    let operand = if boxed {
        ctx.emit_value(
            Op::MixedBox,
            vec![source],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(call.span),
        )
        .value
    } else {
        source
    };
    let result = ctx.emit_value(
        Op::NormalizeCallable,
        vec![operand],
        Some(Immediate::I64(if boxed { mode | MODE_OWNS_OPERAND_BOX } else { mode })),
        PhpType::Callable,
        Op::NormalizeCallable.default_effects(),
        Some(call.span),
    );
    if boxed {
        ctx.emit_void(
            Op::Release,
            vec![operand],
            None,
            Op::Release.default_effects(),
            Some(call.span),
        );
    }
    Ok(result)
}
