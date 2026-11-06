//! Purpose:
//! Home of the internal `__elephc_opcache_rt_blacklist_entry` builtin: the Nth pattern
//! loaded from `opcache.blacklist_filename`.
//!
//! Called from:
//! - The injected OPcache prelude's `opcache_get_configuration()` body, which lists the
//!   resolved patterns under the `blacklist` key exactly as reference PHP does.
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - Answers the EMPTY string for an out-of-range index. A blacklist entry is never empty
//!   (blank lines are skipped when the file is read), so the prelude uses that as its
//!   loop's safety net, the same way the cached-script loop does.
//! - The bridge hands back BORROWED bytes from a thread-local buffer; the lowering copies
//!   them with `__rt_str_persist` before anything else can call the bridge again.
//! - The lowering folds the call to the empty string when this binary has no eval bridge —
//!   see `__elephc_opcache_rt_stat` for why that gate exists. A binary with no dynamic tier
//!   never loads a blacklist, so the empty list it then reports is the truthful answer.

builtin! {
    contract: "__elephc_opcache_rt_blacklist_entry",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtBlacklistEntry,
    ),
}
