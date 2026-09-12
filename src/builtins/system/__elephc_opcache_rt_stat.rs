//! Purpose:
//! Home of the internal `__elephc_opcache_rt_stat` builtin: one figure of the runtime
//! script cache, selected by a key from `crate::opcache::rt_status_keys`.
//!
//! Called from:
//! - The injected OPcache prelude's `opcache_get_status()` body, which pulls the live
//!   counters one key at a time rather than over an array ABI.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - Answers `0` for an unknown key, which is also the empty-cache answer, so a binary
//!   never misreports because a key space moved.
//! - The lowering folds the whole call to `0` when this binary has no eval bridge: with
//!   no dynamic tier there is nothing to report, and emitting the call would drag the
//!   interpreter archive into a program whose only sin was calling
//!   `opcache_get_status()`.

builtin! {
    contract: "__elephc_opcache_rt_stat",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtStat,
    ),
}
