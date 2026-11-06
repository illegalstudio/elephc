//! Purpose:
//! Home of the internal `__elephc_opcache_rt_script_path` builtin: the canonical path of
//! the Nth script held by the runtime script cache.
//!
//! Called from:
//! - The injected OPcache prelude's `opcache_get_status()` body, which needs the path
//!   both as the `scripts` map's KEY and as each entry's `full_path`.
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - Answers the EMPTY string for an out-of-range index. A real cached path is never
//!   empty, so the prelude uses that as its loop's safety net.
//! - The bridge hands back BORROWED bytes from a thread-local buffer; the lowering copies
//!   them with `__rt_str_persist` before anything else can call the bridge again.
//! - The lowering folds the call to the empty string when this binary has no eval bridge —
//!   see `__elephc_opcache_rt_stat` for why that gate exists.

builtin! {
    contract: "__elephc_opcache_rt_script_path",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtScriptPath,
    ),
}
