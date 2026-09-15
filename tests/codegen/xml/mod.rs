//! Purpose:
//! Groups the end-to-end `ext/xml` / `ext/xmlwriter` codegen fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests xml` through Rust's test harness.
//!
//! Key details:
//! - Every fixture links the `elephc_xml` bridge through the injected prelude, and with it
//!   the managed native `libxml2` 2.15.3 artifact (shim + `libxml2.a`) from this machine's
//!   cache; expectations were pinned against PHP 8.5.10 (libxml2 2.15.3) output.
//! - Each fixture starts with `skip_without_xml_native(...)`: without the artifact it
//!   prints a skip and returns, and under `ELEPHC_TEST_REQUIRE_XML_NATIVE=1` (every CI
//!   shard) a missing artifact panics with the `elephc native add libxml2` recovery.

mod clone;
mod divergences;
mod eval;
mod handlers;
mod into_struct;
mod parser;
mod parser_arguments;
mod writer;
