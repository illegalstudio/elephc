//! Purpose:
//! Emits the `__rt_gen_*` runtime helpers backing the `Generator` built-in
//! class. Generators are stackful coroutines that reuse the Fiber runtime; the
//! helper bodies live in `coro` (the `yield` suspension primitive plus the
//! `current`/`key`/`valid`/`next`/`send`/`throw`/`rewind`/`getReturn`
//! accessors).
//!
//! Called from:
//!  - `crate::codegen::runtime::emitters::emit_runtime()` (once per build).
//!  - User PHP method calls on `Generator` are routed to the emitted
//!    `__rt_gen_*` symbols by the EIR generator intrinsic lowering and the
//!    foreach iterator fast path.
//!
//! Key details:
//!  - A `Generator` object reuses the Fiber 232-byte layout plus the
//!    generator-specific fields defined in `coro`; the helpers drive the
//!    coroutine through `__rt_fiber_*` primitives.

pub(crate) mod coro;
pub(crate) mod frame;

use crate::codegen::emit::Emitter;

/// Emits all `__rt_gen_*` runtime helpers for the current target.
///
/// Emits the fiber-backed `yield` suspension primitive (`__rt_gen_suspend`)
/// followed by the `Generator` method accessors. Both are target-aware
/// internally.
pub(crate) fn emit_generator_runtime(emitter: &mut Emitter) {
    coro::emit_gen_suspend(emitter);
    coro::emit_gen_accessors(emitter);
}
