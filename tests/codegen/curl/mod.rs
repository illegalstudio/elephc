//! Purpose:
//! Groups the end-to-end `ext/curl` codegen fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - Every fixture here links the `elephc_curl` bridge, which in turn needs the managed
//!   native `curl`/`openssl`/`zlib` archives. `super::support::curl_native::available()`
//!   reports whether this machine has them installed; fixtures skip with a printed
//!   message when it does not, so a checkout without `elephc native add curl` still runs
//!   the rest of the suite.

mod constants;
mod easy_handle;
mod easy_http;
mod easy_options;
mod http_fixture;
