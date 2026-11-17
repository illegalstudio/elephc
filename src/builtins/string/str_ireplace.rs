//! Purpose:
//! Home of the PHP `str_ireplace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The by-reference `$count` (fourth argument) is routed to
//!   `crate::str_replace_count_prelude`, exactly as `str_replace`'s is.
//! - php's return type is subject-shaped exactly as `str_replace`'s is
//!   (`str_ireplace(['A','B'], ['x','y'], ['ab','BA'])` answers `['xy', 'yx']`), and the same
//!   backend path serves both, so the two share one result-type resolver.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinEffects, BuiltinResultType, BuiltinSemanticInput,
    BuiltinSemantics,
};

builtin! {
    contract: "str_ireplace",
    semantics: str_ireplace_semantics(),
}

/// Builds runtime semantics whose result follows php's subject-shaped return type.
const fn str_ireplace_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::StrIreplace);
    semantics.result_type =
        BuiltinResultType::Shared(super::str_replace::subject_shaped_result_type);
    semantics.effects = BuiltinEffects::Shared(str_ireplace_effects);
    semantics
}

/// The three-argument form is pure; the by-reference `$count` form writes a caller variable,
/// exactly as `str_replace`'s does, so the optimizer must not treat that call as pure.
fn str_ireplace_effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    let base = crate::ir::RuntimeFnId::StrIreplace.effects();
    if input.args.len() >= 4 {
        base | crate::ir::Effects::WRITES_HEAP | crate::ir::Effects::WRITES_LOCAL
    } else {
        base
    }
}
