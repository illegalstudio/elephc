//! Purpose:
//! Home of the internal `__elephc_opcache_rt_script_field` builtin: one numeric field of
//! the Nth script held by the runtime script cache.
//!
//! Called from:
//! - The injected OPcache prelude's `opcache_get_status()` body, which walks the cached
//!   scripts by index to build the dynamic half of the `scripts` map.
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - Answers `0` for an out-of-range index or an unknown field, matching the empty-cache
//!   answer rather than trapping across the bridge.
//! - The lowering folds the call to `0` when this binary has no eval bridge — see
//!   `__elephc_opcache_rt_stat` for why that gate exists.

builtin! {
    contract: "__elephc_opcache_rt_script_field",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtScriptField,
    ),
}
