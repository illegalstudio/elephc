//! Purpose:
//! Declarative eval registry entries for string-adjacent stream introspection builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings` module loading.
//!
//! Key details:
//! - Runtime behavior stays delegated to existing stream-introspection helpers.

mod stream_get_filters;
mod stream_get_transports;
mod stream_get_wrappers;
mod stream_is_local;
mod stream_supports_lock;
