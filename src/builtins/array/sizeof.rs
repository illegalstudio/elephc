//! Purpose:
//! Home of the PHP `sizeof` builtin: the registry-visible alias of `count`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `sizeof` shares `count`'s checker and typed runtime target (`RuntimeFnId::Count`),
//!   so the registry's alias arity gate requires the two declarations to agree.

builtin! {
    contract: "sizeof",
    check: super::count::check,
    semantics: super::count::count_semantics(),
}
