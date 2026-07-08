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
mod explode;
mod gzcompress;
mod gzdeflate;
mod gzinflate;
mod gzuncompress;
mod hash;
mod hash_algos;
mod hash_copy;
mod hash_file;
mod hash_final;
mod hash_hmac;
mod hash_init;
mod hash_update;
mod implode;
mod md5;
mod sha1;
