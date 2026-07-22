//! Purpose:
//! Home of the PHP `umask` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `umask` is a pure-data builtin whose `Int` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   optional argument and enforces arity before falling back to `returns`.
//! - `arity_error` is overridden to preserve the legacy message
//!   "umask() takes 0 or 1 arguments" (the registry default for a 0-required,
//!   1-optional builtin produces "takes at most 1 argument").

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "umask",
    area: Io,
    params: [mask: Int = DefaultSpec::Null],
    arity_error: "umask() takes 0 or 1 arguments",
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Umask,
    ),
    summary: "Changes the current umask.",
    php_manual: "function.umask",
}
