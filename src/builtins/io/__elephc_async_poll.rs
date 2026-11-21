//! Purpose:
//! Home of the internal Async reactor polling primitive.
//!
//! Called from:
//! - The compiler-injected `Elephc\\Async` prelude through the builtin registry.
//!
//! Key details:
//! - Accepts packed `[fd, interest, ...]` integer entries and a millisecond
//!   timeout; the runtime returns the first ready registration index.
//! - Internal-only so user code cannot couple itself to the v1 reactor ABI.

builtin! {
    contract: "__elephc_async_poll",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcAsyncPoll,
    ),
}
