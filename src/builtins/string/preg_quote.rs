//! Purpose:
//! Home of the PHP `preg_quote` builtin and its backend-neutral runtime target.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - Escaping is pure byte work, so this deliberately does NOT join `uses_regex_runtime()`:
//!   `preg_quote()` compiles and runs without the optional `pcre2` native package, which is
//!   what lets a program quote a pattern it never compiles.
//! - The optional delimiter is declared `string` with a `null` default, the catalog's
//!   established spelling for PHP's `?string $delimiter = null`.

builtin! {
    contract: "preg_quote",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregQuote,
    ),
}
