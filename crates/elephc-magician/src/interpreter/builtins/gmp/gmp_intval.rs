//! Purpose:
//! Declares the Magician binding for `gmp_intval`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Forwards to the compiled GMP prelude helper through the shared native bridge.

use super::*;

eval_builtin! { contract: "gmp_intval", area: Gmp, direct: Gmp, values: GmpIntval }

/// Selects checked projection at the public invocation while leaving the helper's PHP rules intact.
pub(in crate::interpreter) fn eval_gmp_intval_values_result(
    arguments: &[RuntimeCellHandle], context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let helper = if context.semantic_profile().contains(elephc_semantics::StrictFeature::Casts) {
        "__elephc_gmp_checked_intval"
    } else { "__elephc_gmp_intval" };
    dispatch::eval_gmp_helper_values_result("gmp_intval", helper, arguments, context, values)
}
