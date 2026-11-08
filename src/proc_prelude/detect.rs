//! Purpose:
//! Decides whether a parsed program references PHP's `proc_open()` family.
//!
//! Called from:
//! - `crate::proc_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so names are raw source text and PHP function lookup
//!   is case-insensitive. Only the unqualified last segment is compared, which matches
//!   `proc_open`, `\proc_open` and a namespaced fallback call.
//! - The function set is exact (`elephc_builtin_contract::PROC_FUNCTIONS`), never a
//!   prefix: a user `proc_helper()` must not inject the surface or link the bridge.
//! - Soundness beats precision: a missed reference would leave a lowering calling an
//!   undeclared helper, while a false positive only adds prunable declarations.

use crate::parser::ast::Program;

/// Returns whether any statement references a `proc_*` builtin.
pub(super) fn program_uses_proc(program: &Program) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.functions.iter().any(|name| {
        name.rsplit('\\').next().is_some_and(|segment| {
            elephc_builtin_contract::PROC_FUNCTIONS
                .iter()
                .any(|candidate| segment.eq_ignore_ascii_case(candidate))
        })
    })
}
