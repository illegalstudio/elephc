//! Purpose:
//! Home of the internal `__elephc_opcache_rt_reset` builtin: schedules a restart of the
//! runtime script cache and answers whether this call is the one that scheduled it.
//!
//! Called from:
//! - The injected OPcache prelude's `opcache_reset()` body, so a NATIVELY compiled reset
//!   reaches the dynamic tier and not just the reported latch.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - A WRITE, like `__elephc_opcache_rt_swap`: it latches a restart on the process-wide
//!   cache, and its result is the once-then-false answer `opcache_reset()` reports, so the
//!   optimizer must not fold a second call into the first.
//! - SCHEDULES ONLY. php-src performs the restart at the next request, which for elephc is
//!   the `--web` handler's per-request boundary; see `crate::codegen::frame`.
//! - The lowering folds the call to `0` when this binary has no eval bridge: with no
//!   dynamic tier there is nothing to restart, and emitting the call would drag the
//!   interpreter archive into a program whose only sin was calling `opcache_reset()`. The
//!   prelude's own latch still answers, so the REPORTED behaviour is unchanged there.

builtin! {
    contract: "__elephc_opcache_rt_reset",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcOpcacheRtReset,
    ),
}
