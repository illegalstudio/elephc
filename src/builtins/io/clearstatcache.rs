//! Purpose:
//! Home of the PHP `clearstatcache` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `clearstatcache` is a pure-data builtin whose return
//!   type (`Void`) is fully determined by its declaration. The registry common path
//!   infers arguments and enforces arity before falling back to `returns`.
//! - PHP accepts up to 2 optional arguments; elephc has no stat cache but accepts
//!   and ignores them (matching legacy behavior).

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "clearstatcache",
    area: Io,
    params: [
        clear_realpath_cache: Bool = DefaultSpec::Bool(false),
        filename: Str = DefaultSpec::Str("")
    ],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Clearstatcache,
    ),
    summary: "Clears file status cache.",
    php_manual: "function.clearstatcache",
}
