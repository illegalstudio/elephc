//! Purpose:
//! Home of the PHP `str_ireplace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature includes an optional `count` param, but `max_args: 3`
//!   caps arity so only three arguments are accepted, matching PHP's practical use.
//! - php's return type is subject-shaped exactly as `str_replace`'s is
//!   (`str_ireplace(['A','B'], ['x','y'], ['ab','BA'])` answers `['xy', 'yx']`), and the same
//!   backend path serves both, so the two share one result-type resolver.

use crate::builtins::semantics::{runtime_fn_semantics, BuiltinResultType, BuiltinSemantics};

builtin! {
    contract: "str_ireplace",
    semantics: str_ireplace_semantics(),
}

/// Builds runtime semantics whose result follows php's subject-shaped return type.
const fn str_ireplace_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::StrIreplace);
    semantics.result_type =
        BuiltinResultType::Shared(super::str_replace::subject_shaped_result_type);
    semantics
}
