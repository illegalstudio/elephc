//! Purpose:
//! `elephc-curl`: the libcurl-backed bridge staticlib for elephc's `ext/curl`
//! surface. Owns the `i64`-keyed easy-handle table and every
//! `elephc_curl_*` C ABI entry point in `abi.rs`: easy-handle lifecycle,
//! `curl_setopt()` in all its typed forms, performing a transfer,
//! `curl_getinfo()`, the multi and share interfaces, MIME/multipart
//! uploads, and PHP-callable callback dispatch.
//!
//! Called from:
//! - Compiled PHP programs that use curl, via `extern "elephc_curl"`
//!   declarations the curl prelude (`src/curl_prelude.rs`) injects; the
//!   PHP-program linker supplies this crate's staticlib alongside a real
//!   managed `libcurl.a`/`libssl.a`/`libcrypto.a`/`libz.a`.
//! - This crate's own gated tests (`cargo test -p elephc-curl`, see below).
//!
//! Key details:
//! - This crate DECLARES libcurl's `extern "C"` symbols (`easy.rs`) without
//!   linking them at `cargo build -p elephc-curl` time: the `staticlib`/
//!   `rlib` outputs never invoke the system linker. The PHP-program linker
//!   supplies the real `libcurl.a` from a managed native package
//!   (`src/linker/bridges.rs`).
//! - `cargo test -p elephc-curl` DOES produce a real executable, so its
//!   gated real tests (`tests.rs`, behind `#[cfg(elephc_curl_native)]`) only
//!   compile in when `build.rs` detects `ELEPHC_CURL_LIB_DIR` and emits both
//!   the link directives and the cfg. See `build.rs` for the full mechanism.
//! - Module layout: `easy` (raw libcurl bindings), `ca` (preserving native Apple
//!   SecTrust on iOS and runtime CA-file discovery on file-based targets), `handles`
//!   (the easy-handle
//!   table + panic-firewall helpers), `php_layer` (PHP-only option/write-
//!   callback semantics), `callbacks` (PHP-callable write/header/read/
//!   progress/debug callback dispatch), `info` (`curl_getinfo()`'s data
//!   shapes), `mime` (multipart/file uploads), `multi` (the multi
//!   interface), `options` (the frozen `CURLOPT_*` classification table),
//!   `share` (the share interface), `abi` (the `#[no_mangle]` C ABI entry
//!   points every module above is reached through).

mod abi;
mod ca;
mod callbacks;
mod easy;
mod handles;
mod info;
mod mime;
mod multi;
mod monitoring;
mod options;
mod php_layer;
mod share;

#[cfg(test)]
mod tests;
