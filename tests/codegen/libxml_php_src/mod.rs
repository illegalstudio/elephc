//! Purpose:
//! PHP 8.5.8 `ext/libxml` php-src tests rewritten as Elephc codegen tests.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Every test runs inside a mirrored copy of `tests/fixtures/libxml_php_src/`; see
//!   `php_src_corpus.rs`.
//! - The tests link the managed libxml2 artifact and skip through
//!   `skip_without_xml_native`.

pub(super) use super::php_src_corpus::{assert_exact, assert_format};
pub(super) use crate::support::skip_without_xml_native;

/// Root of the php-src libxml fixture assets.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/libxml_php_src");

/// Runs one libxml test program from `folder` with `env` and returns its merged output bytes.
pub(super) fn run_case(folder: &str, env: &[(&str, &str)], source: &str) -> Vec<u8> {
    super::php_src_corpus::run_case_in("libxml", FIXTURES, folder, env, source)
}

mod cases;
