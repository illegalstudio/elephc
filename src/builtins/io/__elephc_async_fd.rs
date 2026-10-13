//! Purpose:
//! Home of the internal Async reactor descriptor-ownership primitive.
//!
//! Called from:
//! - The compiler-injected `Elephc\Async` prelude through the builtin registry.
//!
//! Key details:
//! - Operation zero duplicates a native source descriptor; operation one closes
//!   the reactor-owned duplicate. Both paths return a raw integer status.

builtin! {
    contract: "__elephc_async_fd",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcAsyncFd,
    ),
}
