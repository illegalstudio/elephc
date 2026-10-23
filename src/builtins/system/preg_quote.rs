//! Purpose:
//! Home of the PHP `preg_quote` builtin and its runtime semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Escaping is a pure byte transformation: it needs no regex engine, so the call works
//!   without the optional PCRE2 package and is `PURE`.

builtin! {
    contract: "preg_quote",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregQuote,
    ),
}
