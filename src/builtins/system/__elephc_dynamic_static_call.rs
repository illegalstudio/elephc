//! Purpose:
//! Declares the internal dynamic-receiver method call the parser emits for `$class::method()`.
//!
//! Called from:
//! - `crate::parser::expr::pratt`, which desugars a dynamic receiver followed by `::`, a literal
//!   method name and `(` into `__elephc_dynamic_static_call($class, 'method', ...$args)`.
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
    contract: "__elephc_dynamic_static_call",
    semantics: internal_eir_semantics(
        lower,
        // The call runs an arbitrary user method (which may write globals, the heap, files or
        // output), so passes that read this descriptor before the expansion must treat it as
        // opaquely as an ordinary user call.
        Effects::all(),
        BuiltinResultOwnership::Fresh,
    ),
}

/// Rejects a dynamic-receiver method call that reached registry lowering.
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
