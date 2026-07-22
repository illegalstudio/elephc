//! Purpose:
//! Home of the PHP `strstr` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`haystack`, `needle`,
//!   `before_needle`), but `max_args: 2` caps `check_arity` so a third argument is
//!   rejected, matching the legacy CHECK arm which enforced exactly two arguments.
//! - No `check` hook is needed: the return type (`Str`) is fully determined by the
//!   declaration. The registry dispatch still infers each argument unconditionally, so
//!   undefined-variable diagnostics fire exactly as the legacy arm produced them.


builtin! {
    name: "strstr",
    area: String,
    params: [haystack: Str, needle: Str, before_needle: Bool = crate::builtins::spec::DefaultSpec::Bool(false)],
    max_args: 2,
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strstr,
    ),
    summary: "Returns the portion of a string starting at the first occurrence of a substring.",
    php_manual: "https://www.php.net/manual/en/function.strstr.php",
}
