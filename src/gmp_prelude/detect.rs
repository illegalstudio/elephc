//! Purpose:
//! Decides whether a parsed program references PHP's GMP function or class surface.
//!
//! Called from:
//! - `crate::gmp_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so names are raw source text and PHP function/class
//!   lookup is case-insensitive. Only the unqualified last segment is compared, which
//!   matches `gmp_add`, `\gmp_add` and `\App\gmp_add`.
//! - The function set is exact, never a prefix: `gmdate` and user functions such as
//!   `gmp_helper` must not inject the surface or link the bridge.
//! - Soundness beats precision here: a missed reference would remove declarations a
//!   valid program needs, while a false positive only adds harmless declarations.

use crate::parser::ast::Program;

/// Returns whether any top-level statement references the GMP surface.
pub(super) fn program_uses_gmp(program: &Program) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.functions.iter().any(|name| {
        name.rsplit('\\')
            .next()
            .is_some_and(|segment| elephc_builtin_contract::GMP_FUNCTIONS
                .iter()
                .any(|candidate| segment.eq_ignore_ascii_case(candidate)))
    }) || usage.classes.iter().any(|name| {
        name.rsplit('\\')
            .next()
            .is_some_and(|segment| segment.eq_ignore_ascii_case("GMP"))
    })
}
