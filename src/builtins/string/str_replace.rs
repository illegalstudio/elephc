//! Purpose:
//! Home of the PHP `str_replace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature includes an optional `count` param, but `max_args: 3`
//!   caps arity so only three arguments are accepted, matching PHP's practical use.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "str_replace",
    area: String,
    params: [search: Str, replace: Str, subject: Str, count: Mixed = DefaultSpec::Null],
    max_args: 3,
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrReplace,
    ),
    summary: "Replaces all occurrences of a search string with a replacement string.",
    php_manual: "https://www.php.net/manual/en/function.str-replace.php",
}
