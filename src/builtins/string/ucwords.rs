//! Purpose:
//! Home of the PHP `ucwords` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`string`, `separators`),
//!   but `max_args: 1` caps `check_arity` so a second argument is rejected, matching the
//!   legacy CHECK arm which enforced exactly one argument.
//! - No `check` hook is needed: the return type (`Str`) is fully determined by the
//!   declaration. The registry dispatch still infers each argument unconditionally, so
//!   undefined-variable diagnostics fire exactly as the legacy arm produced them.


builtin! {
    name: "ucwords",
    area: String,
    params: [string: Str, separators: Str = crate::builtins::spec::DefaultSpec::Str(" \t\r\n\u{0c}\u{0b}")],
    max_args: 1,
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Ucwords,
    ),
    summary: "Uppercases the first character of each word in a string.",
    php_manual: "https://www.php.net/manual/en/function.ucwords.php",
}
