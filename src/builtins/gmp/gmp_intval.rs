//! Purpose:
//! Home of the PHP `gmp_intval` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - Operand coercion and php-src diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_intval",
    semantics: semantics(),
}

/// Keeps the shared GMP operand contract while selecting its checked integer projection.
const fn semantics() -> crate::builtins::semantics::BuiltinSemantics {
    let mut semantics = super::int_semantics();
    semantics.lowering = crate::builtins::semantics::BuiltinLowering::Eir(lower);
    semantics
}

/// Checks original float and canonical GMP magnitude before an integer result is published.
fn lower(ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
    call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>)
    -> Result<crate::builtins::semantics::LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    let helper = if crate::semantics::conversions::checked_casts(call.semantic_profile, call.source_mode) {
        "__elephc_gmp_checked_intval"
    } else { "__elephc_gmp_intval" };
    super::lower_with_helper(ctx, call, helper)
}
