//! Purpose:
//! Home of the internal `__elephc_opcache_rt_soft_invalidate` builtin: the runtime script cache's
//! NON-FORCED invalidation, reached by path.
//!
//! Called from:
//! - The injected OPcache prelude, on the `$force === false` arm of `opcache_invalidate()`.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - The SIBLING of `__elephc_opcache_rt_discard`, split by force rather than taking a flag.
//!   php-src's predicate is `force || !validate_timestamps || the source moved on`, and the
//!   prelude can answer none of the last two: the timestamp comparison needs the mtime the
//!   cache ENTRY recorded. Keeping the predicate behind the bridge is what stops the native
//!   and eval surfaces answering differently for one file.
//! - The lowering folds the call to `0` when this binary has no eval bridge, as every
//!   sibling does: with no dynamic tier there is nothing to retire.

builtin! {
    contract: "__elephc_opcache_rt_soft_invalidate",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtSoftInvalidate,
    ),
}
