//! Purpose:
//! Home of the internal `__elephc_closure_from_callable_type_error` builtin: PHP's
//! `Closure::fromCallable($callable)` over a value only known at run time.
//!
//! Called from:
//! - The name resolver's non-literal `Closure::fromCallable()` rewrite.
//!
//! Key details:
//! - Shares checking and lowering with `__elephc_closure_from_callable`; only the rejection
//!   differs, because PHP's `Closure::fromCallable()` throws `TypeError` where `$callable(...)`
//!   throws `Error`.

use crate::builtins::callables::__elephc_closure_from_callable::{
    check_closure_conversion, lower_closure_conversion, MODE_FROM_CALLABLE,
};
use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Effects;

builtin! {
    contract: "__elephc_closure_from_callable_type_error",
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

/// Lowers `Closure::fromCallable($callable)`.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    lower_closure_conversion(ctx, call, MODE_FROM_CALLABLE)
}
