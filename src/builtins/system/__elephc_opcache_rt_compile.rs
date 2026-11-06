//! Purpose:
//! Home of the internal `__elephc_opcache_rt_compile` builtin: the runtime script cache's
//! compile operation, reached by path. Answers `0` for a file that does not parse.
//!
//! Called from:
//! - The injected OPcache prelude, so a NATIVELY compiled `opcache_*` body answers about the
//!   dynamic tier instead of only the compile-time manifest.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - Takes the path as a PHP string, which is what makes the native and eval surfaces agree
//!   on a DYNAMICALLY included file — the manifest cannot answer for one.
//! - This is the one `rt_*` helper whose pay-for-use fold would be a lie: its siblings
//!   `_is_cached` and `_discard` may fold to `false` without a dynamic tier, because nothing
//!   can have been cached there, but `opcache_compile_file()`'s job is to CREATE the entry,
//!   and reference PHP answers `true` and caches the file whether or not the program uses
//!   `eval()`. So calling it LINKS the eval bridge — see
//!   `crate::ir_lower::program::runtime_features`, which also links it for the file-cache
//!   operations under a configured `opcache.file_cache`. Measured at 2.5 MB against a 69 KB
//!   disabled build, and it lands only on programs that both call it AND enable OPcache,
//!   since the prelude's own gate short-circuits before the call is emitted otherwise.

builtin! {
    contract: "__elephc_opcache_rt_compile",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtCompile,
    ),
}
