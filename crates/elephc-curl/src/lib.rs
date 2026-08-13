//! Purpose:
//! `elephc-curl`: the libcurl-backed bridge staticlib for elephc's `ext/curl`
//! surface. Task 3 (this crate's first landing) owns the `i64` easy-handle
//! table and the ten-function C ABI in `abi.rs`: init/set_url/setopt_long
//! (`RETURNTRANSFER`)/perform/errno/error/take_body/free/global_info. Task 7
//! adds one more (`elephc_curl_easy_getinfo_long`, the first `curl_getinfo()`
//! entry point — `CURLINFO_HTTP_CODE` only; Task 8 Wave C owns the rest).
//! Later tasks add multi/share/slist/blob/callback entry points to the same
//! crate.
//!
//! Called from:
//! - Compiled PHP programs that use curl, via `extern "elephc_curl"`
//!   declarations the curl prelude injects (Task 5+); today, only this
//!   crate's own tests (`cargo test -p elephc-curl`).
//!
//! Key details:
//! - This crate DECLARES libcurl's `extern "C"` symbols (`easy.rs`) without
//!   linking them at `cargo build -p elephc-curl` time: the `staticlib`/
//!   `rlib` outputs never invoke the system linker. The PHP-program linker
//!   supplies the real `libcurl.a` (Task 4).
//! - `cargo test -p elephc-curl` DOES produce a real executable, so its
//!   gated real tests (`tests.rs`, behind `#[cfg(elephc_curl_native)]`) only
//!   compile in when `build.rs` detects `ELEPHC_CURL_LIB_DIR` and emits both
//!   the link directives and the cfg. See `build.rs` for the full mechanism.
//! - Module layout: `easy` (raw libcurl bindings), `handles` (the handle
//!   table + panic-firewall helpers), `php_layer` (PHP-only option/write-
//!   callback semantics), `abi` (the `#[no_mangle]` C ABI entry points).

mod abi;
mod easy;
mod handles;
mod info;
mod options;
mod php_layer;

#[cfg(test)]
mod tests;
