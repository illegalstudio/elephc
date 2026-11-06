//! Purpose:
//! Home of the internal `__elephc_opcache_rt_swap` builtin: installs one runtime script
//! cache directive by id and answers the value it replaced.
//!
//! Called from:
//! - The injected OPcache prelude's `ini_set()` body, for the three `opcache.*` directives
//!   php-src registers as `PHP_INI_ALL` and elephc's cache actually reads.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - The only `rt_*` builtin that WRITES. Its effects carry `WRITES_GLOBAL` so the
//!   optimizer cannot fold two `ini_set()` calls together or drop one whose result is
//!   discarded — which is how `ini_set()` is usually written.
//! - The lowering folds the call away when this binary has no eval bridge: with no
//!   dynamic tier there is no cache to configure, and emitting the call would drag the
//!   interpreter archive into a program whose only sin was calling `ini_set()`. The
//!   prelude keeps the REPORTED value in its own override store either way, so
//!   `ini_get()` still moves in a binary that folds this to nothing.

builtin! {
    contract: "__elephc_opcache_rt_swap",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtSwap,
    ),
}
