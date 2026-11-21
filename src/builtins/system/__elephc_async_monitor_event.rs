//! Purpose:
//! Home of the internal Async scheduler monitoring event primitive.
//!
//! Called from:
//! - The compiler-injected `Elephc\\Async` prelude through the builtin registry.
//!
//! Key details:
//! - The emitted runtime wrapper is dormant-gated before it reaches the optional profiler hook.
//! - Numeric state and wake-reason codes are owned by `elephc-monitoring-contract`.

builtin! {
    contract: "__elephc_async_monitor_event",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcAsyncMonitorEvent,
    ),
}
