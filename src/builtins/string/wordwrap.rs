//! Purpose:
//! Home of the PHP `wordwrap` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `string` param plus optional `width`, `break`, and
//!   `cut_long_words` params with PHP-compatible defaults. The `break` param
//!   uses the raw identifier `r#break` because `break` is a Rust keyword.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "wordwrap",
    area: String,
    params: [
        string: Str,
        width: Int = DefaultSpec::Int(75),
        r#break: Str = DefaultSpec::Str("\n"),
        cut_long_words: Bool = DefaultSpec::Bool(false)
    ],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Wordwrap,
    ),
    summary: "Wraps a string to a given number of characters.",
    php_manual: "https://www.php.net/manual/en/function.wordwrap.php",
}
