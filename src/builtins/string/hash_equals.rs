//! Purpose:
//! Home of the PHP `hash_equals` builtin: single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook is needed: `returns: Bool` expresses the return type inline and no
//!   bridge library is required (this is a pure timing-safe byte comparison).
//! - Arity (exactly 2 args) is validated by the registry.


builtin! {
    name: "hash_equals",
    area: String,
    params: [known_string: Str, user_string: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HashEquals,
    ),
    summary: "Compares two strings using a constant-time algorithm.",
    php_manual: "https://www.php.net/manual/en/function.hash-equals.php",
}
