//! Purpose:
//! Home of the internal Async scope cycle-collection safe point.
//!
//! Called from:
//! - The compiler-injected `Elephc\Async\run()` wrapper after scheduler teardown.
//!
//! Key details:
//! - Maps to the existing cycle collector and is not exposed as a PHP extension API.

builtin! {
    contract: "__elephc_async_gc_collect",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcAsyncGcCollect,
    ),
}
