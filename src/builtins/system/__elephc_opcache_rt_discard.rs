//! Purpose:
//! Home of the internal `__elephc_opcache_rt_discard` builtin: the runtime script cache's
//! discard operation, reached by path.
//!
//! Called from:
//! - The injected OPcache prelude, so a NATIVELY compiled `opcache_*` body answers about the
//!   dynamic tier instead of only the compile-time manifest.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - Takes the path as a PHP string, which is what makes the native and eval surfaces agree
//!   on a DYNAMICALLY included file — the manifest cannot answer for one.
//! - The lowering folds the call to `0` when this binary has no eval bridge: with no dynamic
//!   tier there is nothing to ask about, and emitting the call would drag the interpreter
//!   archive into a program whose only sin was calling an `opcache_*` function.

builtin! {
    contract: "__elephc_opcache_rt_discard",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtDiscard,
    ),
}
