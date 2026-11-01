//! Purpose:
//! Home of the PHP `ucwords` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature is PHP's own `ucwords(string $string, string $separators = " \t\r\n\f\v")`.
//!   `$separators` is a byte SET, not a substring: every byte listed ends a word, and the
//!   backend passes the default set explicitly when the argument is omitted.
//! - No `check` hook is needed: the return type (`Str`) is fully determined by the
//!   declaration. The registry dispatch still infers each argument unconditionally, so
//!   undefined-variable diagnostics fire exactly as the legacy arm produced them.


builtin! {
    contract: "ucwords",
    semantics: semantics(),
}

/// A string source can select `ucwords` at run time (`$function($value)`), which Laravel's
/// `Str::plural()` does to match the case of the word it inflects.
const fn semantics() -> crate::builtins::semantics::BuiltinSemantics {
    let mut semantics =
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Ucwords);
    semantics.callable = crate::builtins::semantics::BuiltinCallablePolicy::Dynamic(
        crate::builtins::semantics::callable_accepts_string_source,
    );
    semantics
}
