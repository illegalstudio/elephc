//! Purpose:
//! Home of the internal `__elephc_opcache_rt_in_file_cache` builtin: whether the ON-DISK
//! `opcache.file_cache` holds a usable entry for a path.
//!
//! Called from:
//! - The injected OPcache prelude's `opcache_is_script_cached_in_file_cache()` body.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - It answers the same question a READ would: `file_store::contains` applies the same
//!   mtime, size and path validation a load does, so it cannot report an entry a read would
//!   then reject.
//! - Folds to `0` when the binary links no eval bridge, which is also the honest answer
//!   there: without the dynamic tier nothing is ever written to the file cache.

builtin! {
    contract: "__elephc_opcache_rt_in_file_cache",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtInFileCache,
    ),
}
