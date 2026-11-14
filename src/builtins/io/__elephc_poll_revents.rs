//! Purpose:
//! Home of the internal `__elephc_poll_revents` builtin: the compiler-prelude probe
//! returning a stream descriptor's `poll(2)` revents flags.
//!
//! Called from:
//! - `src/io_poll_prelude.rs` through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` leaves prelude-injected code
//!   alone.
//! - The runtime call uses the same descriptor materialization as `stream_select`, then
//!   asks `__rt_stream_poll_revents` for `POLLERR`/`POLLHUP`/`POLLNVAL` without blocking.

builtin! {
    contract: "__elephc_poll_revents",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPollRevents,
    ),
}
