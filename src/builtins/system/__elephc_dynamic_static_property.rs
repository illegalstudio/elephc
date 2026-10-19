//! Purpose:
//! Declares the internal dynamic-receiver static property read the parser emits for `$class::$prop`.
//!
//! Called from:
//! - `crate::parser::expr::pratt`, which desugars a non-`$this` dynamic receiver followed by
//!   `::` and a member that is not called into `__elephc_dynamic_static_property($class, 'member')`.
//!
//! Key details:
//! - Direct calls are expanded one level up, in
//!   `crate::ir_lower::expr::dynamic_class_access`, into a runtime class-name dispatch over the
//!   AOT class table (exact, case-insensitive name match; objects use their runtime class).
//!   The registry lowering hook below is only a guard for paths that bypass that rewrite.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinResultOwnership, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Effects;

builtin! {
    contract: "__elephc_dynamic_static_property",
    semantics: internal_eir_semantics(
        lower,
        Effects::from_bits_retain(
            Effects::READS_GLOBAL.bits()
                | Effects::READS_HEAP.bits()
                | Effects::ALLOC_HEAP.bits()
                | Effects::REFCOUNT_OP.bits()
                | Effects::MAY_THROW.bits(),
        ),
        BuiltinResultOwnership::Fresh,
    ),
}

/// Rejects a dynamic static property read that reached registry lowering.
///
/// The direct call is always rewritten by `crate::ir_lower::expr::dynamic_class_access`
/// before the registry runs, and the primitive is not callable dynamically, so reaching here
/// means a compiler path bypassed that rewrite.
fn lower(
    _ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(format!(
        "{}() must be expanded by the dynamic class access lowering",
        call.name,
    )))
}
